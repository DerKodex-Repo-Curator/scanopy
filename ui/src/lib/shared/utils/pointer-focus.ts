/**
 * Keeps a click-focused element's focus ring hidden until focus moves.
 *
 * Browsers show no ring for focus that comes from a click, but the first key press after it, even
 * Shift on its own, flips the page into keyboard mode and `:focus-visible` starts matching the
 * element that already has focus. A click anywhere in a focusable container (the table's scroll
 * region, a modal panel) focuses the container, so Shift+clicking a row drew a ring around the
 * whole table.
 *
 * Focus that arrives during a pointer press is tagged with `data-pointer-focus`, and app.css hides
 * the outline for it. The tag goes when focus leaves, so Tab to the next element shows its ring
 * as usual. Focus a click causes after the press (a modal focusing its panel as it opens) is
 * tagged too, since it lands before the next task.
 */
export const POINTER_FOCUS_ATTRIBUTE = 'data-pointer-focus';

/** Start tracking. Returns the cleanup. */
export function trackPointerFocus(doc: Document = document): () => void {
	let pressing = false;
	let release: ReturnType<typeof setTimeout> | undefined;

	const onPointerDown = () => {
		clearTimeout(release);
		pressing = true;
	};
	const onPointerUp = () => {
		release = setTimeout(() => (pressing = false), 0);
	};
	const onFocusIn = (event: FocusEvent) => {
		if (pressing && event.target instanceof Element) {
			event.target.setAttribute(POINTER_FOCUS_ATTRIBUTE, '');
		}
	};
	const onFocusOut = (event: FocusEvent) => {
		if (event.target instanceof Element) event.target.removeAttribute(POINTER_FOCUS_ATTRIBUTE);
	};

	doc.addEventListener('pointerdown', onPointerDown, true);
	doc.addEventListener('pointerup', onPointerUp, true);
	doc.addEventListener('focusin', onFocusIn);
	doc.addEventListener('focusout', onFocusOut);
	return () => {
		clearTimeout(release);
		doc.removeEventListener('pointerdown', onPointerDown, true);
		doc.removeEventListener('pointerup', onPointerUp, true);
		doc.removeEventListener('focusin', onFocusIn);
		doc.removeEventListener('focusout', onFocusOut);
	};
}
