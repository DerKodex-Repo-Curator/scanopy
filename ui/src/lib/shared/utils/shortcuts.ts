/**
 * Keyboard shortcuts as key chips show them. Apple platforms take ⌘ and spell keys as glyphs;
 * everywhere else takes Ctrl and spells keys out.
 */

/** This browser's platform string; empty outside a browser. */
export const currentPlatform = typeof navigator === 'undefined' ? '' : navigator.platform;

export function isApplePlatform(platform: string): boolean {
	return /mac|iphone|ipad/i.test(platform);
}

/**
 * One key as its chip shows it. `Mod` is the shortcut modifier: ⌘ on Apple platforms, Ctrl
 * elsewhere. Any other key passes through unchanged.
 */
export function keyLabel(key: string, platform = currentPlatform): string {
	const apple = isApplePlatform(platform);
	switch (key) {
		case 'Mod':
			return apple ? '⌘' : 'Ctrl';
		case 'Shift':
			return apple ? '⇧' : 'Shift';
		case 'Escape':
			return 'Esc';
		case 'Enter':
			return apple ? '↵' : 'Enter';
		// Delete or Backspace, spelled out: the ⌫ glyph reads poorly at key-chip size.
		case 'Delete':
			return 'DEL';
		default:
			return key;
	}
}

/** A Mod shortcut as one key chip shows it: `⌘F` on Apple platforms, `Ctrl F` elsewhere. */
export function shortcutLabel(key: string, platform = currentPlatform): string {
	const mod = keyLabel('Mod', platform);
	return isApplePlatform(platform) ? `${mod}${key}` : `${mod} ${key}`;
}

/** A Shift shortcut as one key chip shows it: `⇧F` on Apple platforms, `Shift F` elsewhere. */
export function shiftShortcutLabel(key: string, platform = currentPlatform): string {
	const shift = keyLabel('Shift', platform);
	return isApplePlatform(platform) ? `${shift}${key}` : `${shift} ${key}`;
}

/**
 * Whether a keydown is Shift plus `key` (a letter) and no other modifier. Matches on `code` too:
 * with Shift held, `key` is the capital, and on some layouts it is a different character.
 */
export function isShiftShortcut(event: KeyboardEvent, key: string): boolean {
	return (
		event.shiftKey &&
		!event.metaKey &&
		!event.ctrlKey &&
		!event.altKey &&
		(event.key.toUpperCase() === key || event.code === `Key${key}`)
	);
}

/**
 * Whether a keypress landed in something that edits text: an input, textarea, select or
 * contenteditable. Single-key shortcuts skip these so the key still types or moves the cursor.
 */
export function isEditableTarget(target: EventTarget | null): boolean {
	const el = target as HTMLElement | null;
	if (!el || typeof el.tagName !== 'string') return false;
	return el.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName);
}
