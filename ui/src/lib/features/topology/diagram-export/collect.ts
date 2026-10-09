/**
 * Browser-side inputs for a diagram export: node geometry from SvelteFlow's internal nodes, and
 * node icons from the rendered DOM. Run inside the export capture, where `isExporting` turns off
 * culling, so every node is mounted.
 */

import type { InternalNode, Node } from '@xyflow/svelte';
import type { DiagramIcon, DiagramSourceNode } from './model';

/**
 * Absolute position and measured size of every node. `getNodes()` carries neither: positions are
 * parent-relative and measured sizes live on the internal node.
 */
export function collectSourceNodes(
	nodes: Node[],
	getInternalNode: (id: string) => InternalNode | undefined
): DiagramSourceNode[] {
	const sources: DiagramSourceNode[] = [];
	for (const node of nodes) {
		if (node.hidden) continue;
		const internal = getInternalNode(node.id);
		const position = internal?.internals.positionAbsolute ?? node.position;
		const width = internal?.measured?.width ?? node.width ?? node.measured?.width ?? 0;
		const height = internal?.measured?.height ?? node.height ?? node.measured?.height ?? 0;
		sources.push({
			id: node.id,
			type: node.type === 'Container' ? 'Container' : 'Element',
			parentId: node.parentId,
			x: position.x,
			y: position.y,
			width,
			height
		});
	}
	return sources;
}

function bytesToBase64(bytes: Uint8Array): string {
	let binary = '';
	for (let i = 0; i < bytes.length; i += 0x8000) {
		binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
	}
	return btoa(binary);
}

function svgDataUri(svg: string): string {
	return `data:image/svg+xml;base64,${bytesToBase64(new TextEncoder().encode(svg))}`;
}

/** A lucide `<svg>` as standalone markup, with `currentColor` resolved to the drawn colour. */
function serializeInlineSvg(svg: SVGElement): string {
	const clone = svg.cloneNode(true) as SVGElement;
	const color = getComputedStyle(svg).color || '#64748b';
	clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
	clone.setAttribute('width', '24');
	clone.setAttribute('height', '24');
	clone.removeAttribute('class');
	return new XMLSerializer().serializeToString(clone).replaceAll('currentColor', color);
}

async function fetchImageDataUri(src: string): Promise<string | null> {
	if (src.startsWith('data:')) return src;
	try {
		const response = await fetch(src);
		if (!response.ok) return null;
		const blob = await response.blob();
		if (blob.type.includes('svg')) return svgDataUri(await blob.text());
		const bytes = new Uint8Array(await blob.arrayBuffer());
		return `data:${blob.type || 'image/png'};base64,${bytesToBase64(bytes)}`;
	} catch {
		return null;
	}
}

const RASTER_SIZE = 64;

async function rasterizeToPng(dataUri: string): Promise<Uint8Array | undefined> {
	try {
		const image = new Image();
		image.src = dataUri;
		await image.decode();
		const canvas = document.createElement('canvas');
		canvas.width = RASTER_SIZE;
		canvas.height = RASTER_SIZE;
		canvas.getContext('2d')?.drawImage(image, 0, 0, RASTER_SIZE, RASTER_SIZE);
		const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'));
		return blob ? new Uint8Array(await blob.arrayBuffer()) : undefined;
	} catch {
		return undefined;
	}
}

export interface CapturedIcons {
	icons: Map<string, DiagramIcon>;
	/** Icon key per node id. */
	iconKeys: Map<string, string>;
}

/**
 * The first icon each node draws (a container's header icon, a card's service logo), keyed so
 * repeated icons are stored once. `png` also rasterizes each icon, for formats that need bitmaps.
 * A logo that fails to load leaves its node without an icon.
 */
export async function captureNodeIcons(
	flowElement: HTMLElement,
	nodeIds: Set<string>,
	options: { png: boolean }
): Promise<CapturedIcons> {
	const iconKeys = new Map<string, string>();
	const sources = new Map<string, { svg?: string; src?: string }>();

	for (const el of flowElement.querySelectorAll<HTMLElement>('.svelte-flow__node[data-id]')) {
		const id = el.dataset.id;
		if (!id || !nodeIds.has(id)) continue;
		const icon = el.querySelector<HTMLImageElement | SVGElement>('img, svg.lucide-icon');
		if (!icon) continue;
		if (icon instanceof HTMLImageElement) {
			const src = icon.currentSrc || icon.src;
			if (!src) continue;
			iconKeys.set(id, src);
			sources.set(src, { src });
		} else {
			const svg = serializeInlineSvg(icon);
			iconKeys.set(id, svg);
			sources.set(svg, { svg });
		}
	}

	const icons = new Map<string, DiagramIcon>();
	await Promise.all(
		[...sources].map(async ([key, source]) => {
			const dataUri = source.svg ? svgDataUri(source.svg) : await fetchImageDataUri(source.src!);
			if (!dataUri) return;
			icons.set(key, { dataUri, png: options.png ? await rasterizeToPng(dataUri) : undefined });
		})
	);
	for (const [id, key] of iconKeys) if (!icons.has(key)) iconKeys.delete(id);
	return { icons, iconKeys };
}
