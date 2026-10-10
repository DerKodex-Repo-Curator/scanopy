import { describe, it, expect } from 'vitest';
import {
	computeElkLayout,
	portConstraintFor,
	type ElkLayoutInput
} from '$lib/features/topology/layout/elk-layout';
import { buildTopologyParentIndex } from '$lib/features/topology/topology-parent-index';
import type { components } from '$lib/api/schema';

type TopologyNode = components['schemas']['Node'];
type TopologyEdge = components['schemas']['Edge'];

/**
 * The L2 Physical collapse levels 2 and 3, which rendered nothing.
 *
 * `PortOpStatus` subcontainers are `is_subcontainer` *and* `collapsed_by_default`, so in
 * L2 Physical they are closed at every level except 4 while their host containers stay open. That
 * makes levels 2 and 3 the only *mixed* states: some elements are laid out and have pass-1
 * positions, and some are inside a closed subcontainer and have none — yet both still emit ports,
 * because edge endpoints deliberately resolve through the collapsed ancestor.
 *
 * Level 1 (everything collapsed, no positions at all) and level 4 (nothing collapsed, every
 * position present) are uniform and always rendered, which is exactly the reported signature.
 */

function hostContainer(id: string): TopologyNode {
	return {
		id,
		node_type: 'Container',
		container_type: 'Host',
		position: { x: 0, y: 0 },
		size: { x: 400, y: 300 }
	} as TopologyNode;
}

function portStatusSubcontainer(id: string, parent: string): TopologyNode {
	return {
		id,
		node_type: 'Container',
		container_type: 'PortOpStatus',
		parent_container_id: parent,
		position: { x: 0, y: 0 },
		size: { x: 250, y: 40 }
	} as TopologyNode;
}

function interfaceElement(id: string, containerId: string, hostId: string): TopologyNode {
	return {
		id,
		node_type: 'Element',
		element_type: 'Interface',
		host_id: hostId,
		container_id: containerId,
		position: { x: 0, y: 0 },
		size: { x: 250, y: 54 }
	} as TopologyNode;
}

function physicalLink(source: string, target: string): TopologyEdge {
	return {
		edge_type: 'PhysicalLink',
		source,
		target,
		source_handle: 'Bottom',
		target_handle: 'Top',
		view_config: {
			type: 'active' as const,
			affects_layout: true,
			default_visibility: 'visible',
			stroke: 'solid',
			highlight_behavior: 'when_visible',
			will_target_container: false
		}
	} as TopologyEdge;
}

/**
 * Three hosts. `open-host` shows its interface directly, so pass 1 positions it; `closed-host-a`
 * and `closed-host-b` hold theirs inside a collapsed `PortOpStatus` subcontainer, so pass 1 never
 * sees them. Every host is joined to `open-host`, which is what makes all three emit ports.
 */
function mixedCollapseInput(): ElkLayoutInput {
	const nodes: TopologyNode[] = [
		hostContainer('open-host'),
		interfaceElement('open-if', 'open-host', 'open-host'),
		hostContainer('closed-host-a'),
		portStatusSubcontainer('closed-sub-a', 'closed-host-a'),
		interfaceElement('closed-if-a', 'closed-sub-a', 'closed-host-a'),
		hostContainer('closed-host-b'),
		portStatusSubcontainer('closed-sub-b', 'closed-host-b'),
		interfaceElement('closed-if-b', 'closed-sub-b', 'closed-host-b')
	];
	const edges: TopologyEdge[] = [
		physicalLink('closed-if-a', 'open-if'),
		physicalLink('closed-if-b', 'open-if')
	];

	return {
		nodes,
		edges,
		view: 'L2Physical',
		collapsedContainers: new Set(['closed-sub-a', 'closed-sub-b']),
		parentIndex: buildTopologyParentIndex(nodes),
		elementNodeSizes: new Map([
			['open-if', { x: 250, y: 54 }],
			['closed-if-a', { x: 250, y: 54 }],
			['closed-if-b', { x: 250, y: 54 }],
			['closed-sub-a', { x: 250, y: 40 }],
			['closed-sub-b', { x: 250, y: 40 }]
		]),
		topology: {
			id: 'topology',
			nodes,
			edges,
			subnets: [],
			hosts: [],
			ip_addresses: [],
			interfaces: [],
			services: [],
			dependencies: [],
			entity_tags: [],
			ports: [],
			bindings: [],
			tags: [],
			options: {
				local: { hide_edge_types: {}, no_fade_edges: false, hide_resize_handles: false },
				request: {
					hide_ports: false,
					hide_service_categories: [],
					container_rules: [],
					element_rules: []
				}
			}
		} as unknown as ElkLayoutInput['topology']
	};
}

describe('L2 Physical at a mixed collapse level', () => {
	it('gives every rendered container a real size', async () => {
		const result = await computeElkLayout(mixedCollapseInput());

		for (const id of ['open-host', 'closed-host-a', 'closed-host-b']) {
			const size = result.containerSizes.get(id);
			expect(size, `${id} has no size`).toBeDefined();
			expect(size!.width, `${id} width`).toBeGreaterThan(0);
			expect(size!.height, `${id} height`).toBeGreaterThan(0);
		}
	});

	/**
	 * The failure this reproduces is not "no output" but "all output at the origin": ports declared
	 * under `FIXED_POS` with no coordinates are read as (0,0), so every container carrying one is
	 * stacked there and the canvas reads as blank.
	 */
	it('does not stack the containers holding unpositioned ports at the origin', async () => {
		const result = await computeElkLayout(mixedCollapseInput());

		const positions = ['open-host', 'closed-host-a', 'closed-host-b'].map((id) => {
			const pos = result.nodePositions.get(id);
			expect(pos, `${id} has no position`).toBeDefined();
			return `${pos!.x},${pos!.y}`;
		});

		expect(new Set(positions).size, `containers overlap: ${positions.join(' | ')}`).toBe(
			positions.length
		);
	});

	/** The two uniform levels, which always rendered, must keep doing so. */
	it('still lays out the uniform levels either side of it', async () => {
		const everythingClosed = mixedCollapseInput();
		everythingClosed.collapsedContainers = new Set([
			'open-host',
			'closed-host-a',
			'closed-host-b',
			'closed-sub-a',
			'closed-sub-b'
		]);
		const nothingClosed = mixedCollapseInput();
		nothingClosed.collapsedContainers = new Set();

		for (const input of [everythingClosed, nothingClosed]) {
			const result = await computeElkLayout(input);
			for (const id of ['closed-host-a', 'closed-host-b']) {
				const size = result.containerSizes.get(id);
				expect(size?.width ?? 0).toBeGreaterThan(0);
				expect(size?.height ?? 0).toBeGreaterThan(0);
			}
		}
	});
});

describe('portConstraintFor', () => {
	it('fixes positions only when every port on the container has them', () => {
		expect(
			portConstraintFor([
				{ x: 10, y: 20 },
				{ x: 0, y: 40 }
			])
		).toBe('FIXED_POS');
	});

	/**
	 * The mixed collapse state in one assertion: one element inside a collapsed container has no
	 * pass-1 position, so its port has no coordinates, and the container it shares with positioned
	 * siblings must fall back rather than let ELK read the gap as the origin.
	 */
	it('falls back to the side constraint when any single port is unpositioned', () => {
		expect(portConstraintFor([{ x: 10, y: 20 }, {}])).toBe('FIXED_SIDE');
		expect(portConstraintFor([{}, {}])).toBe('FIXED_SIDE');
	});

	/** A half-known coordinate is not a coordinate — ELK would read the missing axis as zero. */
	it('does not accept a port with only one axis', () => {
		expect(portConstraintFor([{ x: 10 }])).toBe('FIXED_SIDE');
		expect(portConstraintFor([{ y: 10 }])).toBe('FIXED_SIDE');
	});
});
