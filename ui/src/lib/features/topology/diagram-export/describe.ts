/**
 * Text and colours for each node and edge of a diagram export, read from the same sources the
 * canvas components use (`ElementNode`, `ContainerNode`, `CustomEdge`), plus the entity fields each
 * shape carries as data.
 */

import type { Edge } from '@xyflow/svelte';
import { containerTypes, edgeTypes, entities } from '$lib/shared/stores/metadata';
import { hostDisplayName } from '$lib/features/hosts/host-display-name';
import {
	common_cidr,
	common_host,
	common_id,
	common_interface,
	common_ipAddress,
	common_macAddress,
	common_services,
	common_type
} from '$lib/paraglide/messages';
import { elementEntity, resolveElementNode } from '../resolvers';
import { createColorHelper, type Color } from '$lib/shared/utils/styling';
import { buildElementRender, currentElementRenderContext } from '../element-render-data';
import { formatElementSummary, tallyContainerElements } from '../labels';
import { getInfrastructureRuleIdForTopology } from '../queries';
import {
	isDottedEdge,
	isEdgeHidden,
	isOverlayEdge,
	restingEdgeStrokeWidth
} from '../layout/edge-classification';
import type { RenderableTopology, TopologyEdge, TopologyNode } from '../types/base';
import type {
	DiagramDescriber,
	DiagramEdgeDescription,
	DiagramField,
	DiagramHeader,
	DiagramNodeDescription,
	DiagramSourceNode
} from './model';

export type DiagramTheme = 'light' | 'dark';

/**
 * Opaque equivalents of the canvas colours (`app.css` topology variables and the export-only
 * card borders in `topology-viewer.css`). Visio has no alpha on fills, so the translucent
 * subgroup background is pre-blended over the node background.
 */
export const DIAGRAM_PALETTE: Record<
	DiagramTheme,
	{
		canvas: string;
		node: string;
		subgroup: string;
		border: string;
		text: string;
		mutedText: string;
	}
> = {
	light: {
		canvas: '#f8fafc',
		node: '#ffffff',
		subgroup: '#f7f7f7',
		border: '#94a3b8',
		text: '#1f2937',
		mutedText: '#64748b'
	},
	dark: {
		canvas: '#15131e',
		node: '#1a1d29',
		subgroup: '#232633',
		border: '#6b7280',
		text: '#f3f4f6',
		mutedText: '#9ca3af'
	}
};

export function diagramHeader(theme: DiagramTheme, title: string, subtitle: string): DiagramHeader {
	const palette = DIAGRAM_PALETTE[theme];
	return { title, subtitle, color: palette.text, subtitleColor: palette.mutedText };
}

export interface DescriberInput {
	topology: RenderableTopology;
	/** Flow node data (the backend node, plus `isCollapsed` when collapsed), by node id. */
	nodeData: Map<string, TopologyNode & { isCollapsed?: boolean }>;
	/** Flow edges by id, for their data. */
	edges: Map<string, Edge>;
	hiddenEdgeTypes: readonly string[];
	theme: DiagramTheme;
	/** Icon key per node id, for nodes whose icon was captured. */
	iconKeys: Map<string, string>;
}

function elementDescription(
	id: string,
	node: TopologyNode,
	topology: RenderableTopology
): { title: string; lines: string[] } {
	const { data } = buildElementRender({
		nodeId: id,
		node,
		topology,
		...currentElementRenderContext()
	});
	if (!data) return { title: node.header ?? '', lines: [] };
	const title = data.headerText ?? data.subtitleText ?? '';
	const lines: string[] = [];
	if (data.headerText && data.subtitleText) lines.push(data.subtitleText);
	if (data.showServices) for (const service of data.services) lines.push(service.name);
	for (const group of data.inlineGroups) {
		for (const service of group.services) lines.push(service.name);
	}
	if (data.bodyText) lines.push(data.bodyText);
	if (data.footerText) lines.push(data.footerText);
	return { title, lines };
}

/** Keep only fields with a value, so a shape's data lists what is known about it. */
function fields(entries: [string, string | null | undefined][]): DiagramField[] {
	return entries
		.filter((entry): entry is [string, string] => !!entry[1]?.trim())
		.map(([label, value]) => ({ label, value }));
}

function elementFields(
	id: string,
	node: TopologyNode,
	topology: RenderableTopology
): DiagramField[] {
	const resolved = resolveElementNode(id, node, topology);
	const iface = resolved.snmpInterface;
	const subnet = topology.subnets.find((s) => s.id === resolved.subnetId);
	return fields([
		[common_host(), resolved.host ? hostDisplayName(resolved.host) : null],
		[common_ipAddress(), resolved.ipAddress?.ip_address],
		[common_macAddress(), resolved.ipAddress?.mac_address ?? iface?.mac_address],
		[common_interface(), iface?.if_name ?? iface?.if_descr],
		[entities.getName('Subnet'), subnet?.cidr],
		[common_services(), resolved.services.map((s) => s.name).join(', ')],
		[common_id(), elementEntity(resolved)?.id]
	]);
}

function containerFields(
	id: string,
	node: TopologyNode & { node_type: 'Container' },
	topology: RenderableTopology
): DiagramField[] {
	const containerType = node.container_type ?? 'Subnet';
	const entityId = node.entity_id ?? id;
	const subnet =
		containerType === 'Subnet' ? topology.subnets.find((s) => s.id === entityId) : undefined;
	return fields([
		[common_type(), containerTypes.getName(containerType)],
		[common_cidr(), subnet?.cidr],
		[common_id(), node.entity_id]
	]);
}

export function createDescriber(input: DescriberInput): DiagramDescriber {
	const palette = DIAGRAM_PALETTE[input.theme];
	const infraRuleId = getInfrastructureRuleIdForTopology(input.topology);
	const activeView = currentElementRenderContext().activeView;

	return {
		node(source: DiagramSourceNode): DiagramNodeDescription {
			const data = input.nodeData.get(source.id);
			const iconKey = input.iconKeys.get(source.id);
			if (!data || data.node_type !== 'Container') {
				const { title, lines } = data
					? elementDescription(source.id, data, input.topology)
					: { title: '', lines: [] };
				return {
					title,
					lines,
					iconKey,
					data: data ? elementFields(source.id, data, input.topology) : [],
					style: { fill: palette.node, stroke: palette.border, text: palette.text }
				};
			}

			const containerType = data.container_type ?? 'Subnet';
			const isSubcontainer = containerTypes.getMetadata(containerType)?.is_subcontainer ?? false;
			const accent = data.color
				? createColorHelper(data.color as Color).rgb
				: containerTypes.getColorHelper(containerType).rgb;
			const lines = data.isCollapsed
				? [
						formatElementSummary(
							tallyContainerElements(source.id, input.topology, infraRuleId),
							activeView
						)
					].filter(Boolean)
				: [];
			return {
				title: data.header ?? '',
				lines,
				iconKey,
				data: containerFields(source.id, data, input.topology),
				style: {
					fill: isSubcontainer ? palette.subgroup : palette.node,
					stroke: palette.border,
					text: palette.text,
					accent: isSubcontainer ? undefined : accent
				}
			};
		},

		edge(source): DiagramEdgeDescription | null {
			const flowEdge = input.edges.get(source.id);
			const data = flowEdge?.data as
				(TopologyEdge & Record<string, unknown> & { is_preview?: boolean }) | undefined;
			// A preview edge is an unsaved dependency draft in the editor, not part of the topology.
			if (data?.is_preview) return null;
			if (
				isEdgeHidden(data, {
					isPreview: false,
					simplified: false,
					hiddenEdgeTypes: input.hiddenEdgeTypes
				})
			) {
				return null;
			}

			const isBundle = !!data?.isBundle;
			const isOverlay = isBundle ? !!data?.bundleIsOverlay : data ? isOverlayEdge(data) : false;
			const isDotted = !isBundle && data ? isDottedEdge(data) : false;
			const isDependencyEdge = data
				? (edgeTypes.getMetadata(data.edge_type)?.is_dependency_edge ?? false)
				: false;
			const dependency =
				data && 'dependency_id' in data
					? input.topology.dependencies?.find((d) => d.id === data.dependency_id)
					: undefined;
			const color = dependency?.color
				? createColorHelper(dependency.color).rgb
				: data
					? edgeTypes.getColorHelper(data.edge_type).rgb
					: createColorHelper('Gray').rgb;

			return {
				label: typeof flowEdge?.label === 'string' ? flowEdge.label : undefined,
				color,
				width: restingEdgeStrokeWidth({
					isAggregated: !!data?.isAggregated,
					aggregatedCount: (data?.aggregatedCount as number) ?? 1,
					isBundle,
					bundleStrokeWidth: (data?.bundleStrokeWidth as number) ?? 2,
					isOverlay
				}),
				dash: !isDependencyEdge && isOverlay ? (isDotted ? 'dotted' : 'dashed') : 'solid',
				data: fields([[common_type(), data ? edgeTypes.getName(data.edge_type) : null]])
			};
		}
	};
}
