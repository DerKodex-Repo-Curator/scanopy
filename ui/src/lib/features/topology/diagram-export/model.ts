/**
 * Format-neutral model of the drawn topology, which the draw.io and Visio serializers both read.
 *
 * It is built from the flow graph as the export capture draws it: the nodes in the store (view,
 * filters and collapse are already applied upstream) and only the edges the canvas draws. Text,
 * colours and icons come from a `DiagramDescriber`, so this module stays free of stores and the
 * structural rules (containment, edge endpoints, what is left out) are testable on their own.
 */

/** A side of a shape, matching the flow handle ids. */
export type DiagramSide = 'Top' | 'Right' | 'Bottom' | 'Left';

export const DIAGRAM_SIDES: readonly DiagramSide[] = ['Top', 'Right', 'Bottom', 'Left'];

/** Space around the content, in flow units, matching the raster exports' crop. */
export const DIAGRAM_PADDING = 40;
/** Height reserved above the content for the header block. */
export const DIAGRAM_HEADER_HEIGHT = 56;

/** A rendered flow node, with its absolute position and measured size. */
export interface DiagramSourceNode {
	id: string;
	type: 'Container' | 'Element';
	parentId?: string;
	x: number;
	y: number;
	width: number;
	height: number;
}

/** A flow edge, as handed to SvelteFlow. */
export interface DiagramSourceEdge {
	id: string;
	source: string;
	target: string;
	sourceHandle?: string | null;
	targetHandle?: string | null;
}

export interface DiagramShapeStyle {
	fill: string;
	stroke: string;
	text: string;
	/** Container accent (the coloured top border on the canvas). */
	accent?: string;
}

export interface DiagramNodeDescription {
	title: string;
	lines: string[];
	style: DiagramShapeStyle;
	/** Key into `DiagramModel.icons`. */
	iconKey?: string;
}

export interface DiagramEdgeDescription {
	label?: string;
	color: string;
	width: number;
	dash: 'solid' | 'dashed' | 'dotted';
}

export interface DiagramDescriber {
	node(node: DiagramSourceNode): DiagramNodeDescription;
	/** `null` when the canvas leaves the edge undrawn. */
	edge(edge: DiagramSourceEdge): DiagramEdgeDescription | null;
}

export interface DiagramIcon {
	/** Image as a data URI (SVG or raster), for formats that embed it directly. */
	dataUri: string;
	/** PNG bytes, for formats that only take bitmaps. */
	png?: Uint8Array;
}

export interface DiagramShape {
	id: string;
	parentId: string | null;
	type: 'container' | 'element';
	/** Absolute, in diagram units, origin at the diagram's top-left. */
	x: number;
	y: number;
	width: number;
	height: number;
	title: string;
	lines: string[];
	style: DiagramShapeStyle;
	iconKey?: string;
}

export interface DiagramConnector {
	id: string;
	sourceId: string;
	targetId: string;
	sourceSide: DiagramSide;
	targetSide: DiagramSide;
	label?: string;
	color: string;
	width: number;
	dash: 'solid' | 'dashed' | 'dotted';
}

export interface DiagramHeader {
	title: string;
	subtitle: string;
	color: string;
	subtitleColor: string;
}

export interface DiagramModel {
	name: string;
	width: number;
	height: number;
	background: string;
	header: DiagramHeader | null;
	/** Parents before their children. */
	shapes: DiagramShape[];
	connectors: DiagramConnector[];
	icons: Map<string, DiagramIcon>;
}

export interface DiagramModelInput {
	name: string;
	nodes: DiagramSourceNode[];
	edges: DiagramSourceEdge[];
	describe: DiagramDescriber;
	background: string;
	header?: DiagramHeader | null;
	icons?: Map<string, DiagramIcon>;
}

function toSide(handle: string | null | undefined, fallback: DiagramSide): DiagramSide {
	return DIAGRAM_SIDES.includes(handle as DiagramSide) ? (handle as DiagramSide) : fallback;
}

/** Order nodes so every parent precedes its children; nodes whose parent is absent become roots. */
function orderParentsFirst(nodes: DiagramSourceNode[]): DiagramSourceNode[] {
	const byId = new Map(nodes.map((n) => [n.id, n]));
	const ordered: DiagramSourceNode[] = [];
	const placed = new Set<string>();
	const place = (node: DiagramSourceNode, trail: Set<string>) => {
		if (placed.has(node.id) || trail.has(node.id)) return;
		trail.add(node.id);
		const parent = node.parentId ? byId.get(node.parentId) : undefined;
		if (parent) place(parent, trail);
		placed.add(node.id);
		ordered.push(node);
	};
	for (const node of nodes) place(node, new Set());
	return ordered;
}

export function buildDiagramModel(input: DiagramModelInput): DiagramModel {
	const nodes = orderParentsFirst(input.nodes);
	const nodeIds = new Set(nodes.map((n) => n.id));
	const header = input.header ?? null;

	// A loop, not Math.min(...): a spread of ~20k arguments is near the engine limits.
	let minX = Infinity;
	let minY = Infinity;
	let maxX = -Infinity;
	let maxY = -Infinity;
	for (const n of nodes) {
		minX = Math.min(minX, n.x);
		minY = Math.min(minY, n.y);
		maxX = Math.max(maxX, n.x + n.width);
		maxY = Math.max(maxY, n.y + n.height);
	}
	if (nodes.length === 0) minX = minY = maxX = maxY = 0;
	const offsetX = DIAGRAM_PADDING - minX;
	const offsetY = DIAGRAM_PADDING + (header ? DIAGRAM_HEADER_HEIGHT : 0) - minY;

	const shapes: DiagramShape[] = nodes.map((node) => {
		const description = input.describe.node(node);
		const parentId = node.parentId && nodeIds.has(node.parentId) ? node.parentId : null;
		return {
			id: node.id,
			parentId,
			type: node.type === 'Container' ? 'container' : 'element',
			x: node.x + offsetX,
			y: node.y + offsetY,
			width: node.width,
			height: node.height,
			title: description.title,
			lines: description.lines,
			style: description.style,
			iconKey: description.iconKey
		};
	});

	const connectors: DiagramConnector[] = [];
	for (const edge of input.edges) {
		if (!nodeIds.has(edge.source) || !nodeIds.has(edge.target)) continue;
		if (edge.source === edge.target) continue;
		const description = input.describe.edge(edge);
		if (!description) continue;
		connectors.push({
			id: edge.id,
			sourceId: edge.source,
			targetId: edge.target,
			sourceSide: toSide(edge.sourceHandle, 'Bottom'),
			targetSide: toSide(edge.targetHandle, 'Top'),
			label: description.label || undefined,
			color: description.color,
			width: description.width,
			dash: description.dash
		});
	}

	return {
		name: input.name,
		width: maxX - minX + DIAGRAM_PADDING * 2,
		height: maxY - minY + DIAGRAM_PADDING * 2 + (header ? DIAGRAM_HEADER_HEIGHT : 0),
		background: input.background,
		header,
		shapes,
		connectors,
		icons: input.icons ?? new Map()
	};
}

/** Where a side's midpoint sits, as a fraction of the shape's width and height. */
export function sideAnchor(side: DiagramSide): { x: number; y: number } {
	switch (side) {
		case 'Top':
			return { x: 0.5, y: 0 };
		case 'Right':
			return { x: 1, y: 0.5 };
		case 'Bottom':
			return { x: 0.5, y: 1 };
		case 'Left':
			return { x: 0, y: 0.5 };
	}
}
