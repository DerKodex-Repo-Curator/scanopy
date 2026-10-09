/**
 * draw.io (mxGraph XML) serializer for a `DiagramModel`.
 *
 * Containers are container cells that hold their children, so moving one in draw.io moves its
 * contents. Edges name their source and target cells, so they stay attached when either moves.
 */

import {
	DIAGRAM_PADDING,
	sideAnchor,
	type DiagramConnector,
	type DiagramField,
	type DiagramModel,
	type DiagramShape
} from './model';
import { escapeXml, fieldKeys, hexColor, num } from './xml';

const ROOT_ID = '0';
const LAYER_ID = '1';

export interface DrawioCell {
	id: string;
	parent: string;
	value: string;
	style: string;
	vertex?: boolean;
	edge?: boolean;
	source?: string;
	target?: string;
	/** Relative to the parent cell for vertices; unused for edges. */
	geometry?: { x: number; y: number; width: number; height: number };
	/** Custom properties, shown in draw.io's Edit Data dialog. */
	data?: { key: string; value: string }[];
}

const shapeCellId = (id: string) => `n-${id}`;
const connectorCellId = (id: string) => `e-${id}`;

function style(entries: Record<string, string | number | undefined>): string {
	return Object.entries(entries)
		.filter(([, v]) => v !== undefined)
		.map(([k, v]) => `${k}=${v}`)
		.join(';');
}

/** draw.io styles use `;` as a separator, so embedded images drop the `;base64` marker. */
function styleImage(dataUri: string): string {
	return dataUri.replace(';base64,', ',');
}

function cellData(fields: DiagramField[]): DrawioCell['data'] {
	if (fields.length === 0) return undefined;
	const keys = fieldKeys(fields.map((f) => f.label));
	return fields.map((f, i) => ({ key: keys[i], value: f.value }));
}

/** HTML label: bold title, then one line per entry. */
function htmlLabel(title: string, lines: string[]): string {
	const parts = [];
	if (title) parts.push(`<b>${escapeXml(title)}</b>`);
	for (const line of lines) parts.push(escapeXml(line));
	return parts.join('<br>');
}

function shapeCell(
	shape: DiagramShape,
	model: DiagramModel,
	byId: Map<string, DiagramShape>
): DrawioCell {
	const parent = shape.parentId ? byId.get(shape.parentId) : undefined;
	const icon = shape.iconKey ? model.icons.get(shape.iconKey) : undefined;
	const isContainer = shape.type === 'container';
	const iconSize = isContainer ? 16 : 20;

	return {
		id: shapeCellId(shape.id),
		parent: parent ? shapeCellId(parent.id) : LAYER_ID,
		value: htmlLabel(shape.title, shape.lines),
		vertex: true,
		data: cellData(shape.data),
		style: style({
			shape: icon ? 'label' : undefined,
			image: icon ? styleImage(icon.dataUri) : undefined,
			imageWidth: icon ? iconSize : undefined,
			imageHeight: icon ? iconSize : undefined,
			imageAlign: icon ? 'left' : undefined,
			imageVerticalAlign: icon ? 'top' : undefined,
			rounded: 1,
			absoluteArcSize: 1,
			arcSize: isContainer ? 12 : 8,
			whiteSpace: 'wrap',
			html: 1,
			container: isContainer ? 1 : undefined,
			collapsible: isContainer ? 0 : undefined,
			recursiveResize: isContainer ? 0 : undefined,
			fillColor: hexColor(shape.style.fill),
			strokeColor: hexColor(shape.style.accent ?? shape.style.stroke),
			strokeWidth: isContainer && shape.style.accent ? 2 : 1,
			fontColor: hexColor(shape.style.text),
			fontSize: isContainer ? 12 : 11,
			align: isContainer || icon ? 'left' : 'center',
			verticalAlign: isContainer || icon ? 'top' : 'middle',
			spacingLeft: icon ? iconSize + 14 : 8,
			spacingTop: 6
		}),
		geometry: {
			x: shape.x - (parent?.x ?? 0),
			y: shape.y - (parent?.y ?? 0),
			width: shape.width,
			height: shape.height
		}
	};
}

function connectorCell(connector: DiagramConnector, background: string): DrawioCell {
	const exit = sideAnchor(connector.sourceSide);
	const entry = sideAnchor(connector.targetSide);
	return {
		id: connectorCellId(connector.id),
		parent: LAYER_ID,
		value: connector.label ? escapeXml(connector.label) : '',
		edge: true,
		data: cellData(connector.data),
		source: shapeCellId(connector.sourceId),
		target: shapeCellId(connector.targetId),
		style: style({
			edgeStyle: 'orthogonalEdgeStyle',
			rounded: 1,
			orthogonalLoop: 1,
			html: 1,
			startArrow: 'none',
			endArrow: 'none',
			strokeColor: hexColor(connector.color),
			strokeWidth: connector.width,
			dashed: connector.dash === 'solid' ? undefined : 1,
			dashPattern:
				connector.dash === 'dotted' ? '1 4' : connector.dash === 'dashed' ? '6 3' : undefined,
			exitX: exit.x,
			exitY: exit.y,
			exitDx: 0,
			exitDy: 0,
			entryX: entry.x,
			entryY: entry.y,
			entryDx: 0,
			entryDy: 0,
			fontSize: 10,
			labelBackgroundColor: hexColor(background)
		})
	};
}

function headerCells(model: DiagramModel): DrawioCell[] {
	if (!model.header) return [];
	const { title, subtitle, color, subtitleColor } = model.header;
	const text = (fontColor: string, fontSize: number, bold: boolean) =>
		style({
			text: '',
			html: 1,
			align: 'left',
			verticalAlign: 'top',
			fontColor: hexColor(fontColor),
			fontSize,
			fontStyle: bold ? 1 : 0,
			strokeColor: 'none',
			fillColor: 'none'
		});
	const width = Math.max(model.width - DIAGRAM_PADDING * 2, 200);
	return [
		{
			id: 'header-title',
			parent: LAYER_ID,
			value: escapeXml(title),
			vertex: true,
			style: text(color, 18, true),
			geometry: { x: DIAGRAM_PADDING, y: DIAGRAM_PADDING - 8, width, height: 26 }
		},
		{
			id: 'header-subtitle',
			parent: LAYER_ID,
			value: escapeXml(subtitle),
			vertex: true,
			style: text(subtitleColor, 12, false),
			geometry: { x: DIAGRAM_PADDING, y: DIAGRAM_PADDING + 18, width, height: 20 }
		}
	];
}

/** The cells of the diagram, root and layer first, parents before children. */
export function toDrawioCells(model: DiagramModel): DrawioCell[] {
	const byId = new Map(model.shapes.map((s) => [s.id, s]));
	return [
		{ id: ROOT_ID, parent: '', value: '', style: '' },
		{ id: LAYER_ID, parent: ROOT_ID, value: '', style: '' },
		...headerCells(model),
		...model.shapes.map((s) => shapeCell(s, model, byId)),
		...model.connectors.map((c) => connectorCell(c, model.background))
	];
}

function serializeCell(cell: DrawioCell): string {
	// A cell with properties is wrapped in <object>, which then owns the id and the label.
	const wrapped = !!cell.data?.length;
	const attrs = wrapped ? [] : [`id="${escapeXml(cell.id)}"`];
	if (cell.parent) attrs.push(`parent="${escapeXml(cell.parent)}"`);
	if (cell.id === ROOT_ID || cell.id === LAYER_ID) return `<mxCell ${attrs.join(' ')}/>`;
	if (!wrapped) attrs.push(`value="${escapeXml(cell.value)}"`);
	attrs.push(`style="${escapeXml(cell.style)}"`);
	if (cell.vertex) attrs.push('vertex="1"');
	if (cell.edge) {
		attrs.push('edge="1"');
		if (cell.source) attrs.push(`source="${escapeXml(cell.source)}"`);
		if (cell.target) attrs.push(`target="${escapeXml(cell.target)}"`);
	}
	const geometry = cell.geometry
		? `<mxGeometry x="${num(cell.geometry.x)}" y="${num(cell.geometry.y)}" width="${num(cell.geometry.width)}" height="${num(cell.geometry.height)}" as="geometry"/>`
		: '<mxGeometry relative="1" as="geometry"/>';
	const mxCell = `<mxCell ${attrs.join(' ')}>${geometry}</mxCell>`;
	if (!wrapped) return mxCell;
	const properties = cell.data!.map((d) => `${d.key}="${escapeXml(d.value)}"`).join(' ');
	return `<object id="${escapeXml(cell.id)}" label="${escapeXml(cell.value)}" ${properties}>${mxCell}</object>`;
}

export function serializeDrawio(model: DiagramModel): string {
	const cells = toDrawioCells(model);
	const body = cells.map(serializeCell).join('\n');
	return [
		'<?xml version="1.0" encoding="UTF-8"?>',
		'<mxfile host="Scanopy" type="device">',
		`<diagram id="scanopy-topology" name="${escapeXml(model.name)}">`,
		`<mxGraphModel dx="0" dy="0" grid="0" gridSize="10" guides="1" tooltips="1" connect="1" arrows="1" fold="1" page="1" pageScale="1" pageWidth="${num(model.width)}" pageHeight="${num(model.height)}" background="${hexColor(model.background)}" math="0" shadow="0">`,
		'<root>',
		body,
		'</root>',
		'</mxGraphModel>',
		'</diagram>',
		'</mxfile>',
		''
	].join('\n');
}
