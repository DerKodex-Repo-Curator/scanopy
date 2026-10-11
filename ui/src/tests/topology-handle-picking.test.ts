import { describe, it, expect } from 'vitest';
import {
	computeOptimalHandles,
	ObstacleGrid,
	type ObstacleRect
} from '$lib/features/topology/layout/handle-picking';

/** Deterministic PRNG so a failure reproduces. */
function rng(seed: number) {
	let s = seed >>> 0;
	return () => {
		s = (s * 1664525 + 1013904223) >>> 0;
		return s / 0x100000000;
	};
}

type Rect = ObstacleRect & { id: string };

/** Cards of mixed sizes plus a few large containers, the mix `getAllNodeRects` returns. */
function randomLayout(seed: number, count: number): Rect[] {
	const r = rng(seed);
	const rects: Rect[] = [];
	for (let i = 0; i < count; i++) {
		rects.push({
			id: `n${i}`,
			x: Math.round(r() * 5000),
			y: Math.round(r() * 3000),
			w: 50 + Math.round(r() * 250),
			h: 30 + Math.round(r() * 150)
		});
	}
	for (let i = 0; i < 5; i++) {
		rects.push({
			id: `c${i}`,
			x: Math.round(r() * 3000),
			y: Math.round(r() * 2000),
			w: 800 + Math.round(r() * 1500),
			h: 500 + Math.round(r() * 1000)
		});
	}
	return rects;
}

describe('ObstacleGrid', () => {
	it('picks the same handles as testing every rect, across random layouts', () => {
		for (let seed = 1; seed <= 20; seed++) {
			const rects = randomLayout(seed, 300);
			const grid = new ObstacleGrid(rects);
			const r = rng(seed * 7919);
			for (let e = 0; e < 50; e++) {
				const src = rects[Math.floor(r() * rects.length)];
				const tgt = rects[Math.floor(r() * rects.length)];
				if (src === tgt) continue;
				// Some edges also exclude an unrelated rect, standing in for an ancestor container.
				const exclude = new Set([src.id, tgt.id]);
				if (r() < 0.5) exclude.add(rects[Math.floor(r() * rects.length)].id);

				const srcPos = { x: src.x, y: src.y };
				const srcSize = { w: src.w, h: src.h };
				const tgtPos = { x: tgt.x, y: tgt.y };
				const tgtSize = { w: tgt.w, h: tgt.h };

				const brute = computeOptimalHandles(
					srcPos,
					srcSize,
					tgtPos,
					tgtSize,
					rects.filter((rect) => !exclude.has(rect.id))
				);
				const indexed = computeOptimalHandles(srcPos, srcSize, tgtPos, tgtSize, (x1, y1, x2, y2) =>
					grid.countCrossings(x1, y1, x2, y2, exclude)
				);
				expect(indexed, `seed ${seed}, ${src.id} -> ${tgt.id}`).toEqual(brute);
			}
		}
	});

	it('counts crossings by segments that run along cell boundaries', () => {
		// A grid of identical cards, so cell edges and card edges line up.
		const rects: Rect[] = [];
		for (let i = 0; i < 10; i++) {
			for (let j = 0; j < 10; j++) {
				rects.push({ id: `${i}-${j}`, x: i * 200, y: j * 200, w: 100, h: 100 });
			}
		}
		const grid = new ObstacleGrid(rects);
		const none = new Set<string>();
		// Horizontal along the top edge of row 3: touches all ten cards in that row.
		expect(grid.countCrossings(0, 600, 1900, 600, none)).toBe(10);
		// Vertical along the left edge of column 2.
		expect(grid.countCrossings(400, 0, 400, 1900, none)).toBe(10);
		// Through the gap between rows: touches nothing.
		expect(grid.countCrossings(0, 150, 1900, 150, none)).toBe(0);
	});
});
