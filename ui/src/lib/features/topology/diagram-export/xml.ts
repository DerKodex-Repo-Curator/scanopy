/**
 * Escape text for an XML attribute value or text node. Also drops characters XML 1.0 forbids,
 * which a device-supplied name can contain.
 */
export function escapeXml(value: string): string {
	return (
		value
			// eslint-disable-next-line no-control-regex
			.replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F\uFFFE\uFFFF]/g, '')
			.replace(/&/g, '&amp;')
			.replace(/</g, '&lt;')
			.replace(/>/g, '&gt;')
			.replace(/"/g, '&quot;')
			.replace(/'/g, '&apos;')
	);
}

/** Round to two decimals so coordinates don't carry float noise into the file. */
export function num(value: number): string {
	return String(Math.round(value * 100) / 100);
}

/** `#RRGGBB` for a CSS hex or `rgb()`/`rgba()` colour (alpha dropped): both formats want hex. */
export function hexColor(css: string): string {
	const value = css.trim();
	const long = value.match(/^#([0-9a-f]{6})/i);
	if (long) return `#${long[1].toUpperCase()}`;
	const short = value.match(/^#([0-9a-f])([0-9a-f])([0-9a-f])$/i);
	if (short) {
		return `#${short[1]}${short[1]}${short[2]}${short[2]}${short[3]}${short[3]}`.toUpperCase();
	}
	const rgb = value.match(/^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/i);
	if (rgb) {
		return `#${[rgb[1], rgb[2], rgb[3]]
			.map((c) => Math.min(255, Number(c)).toString(16).padStart(2, '0'))
			.join('')
			.toUpperCase()}`;
	}
	return '#000000';
}

/**
 * Turn field labels into keys both formats accept: XML attribute names in draw.io, Shape Data row
 * names in Visio. Keys are unique within the list, so two labels that reduce to the same key both
 * survive.
 */
export function fieldKeys(labels: string[]): string[] {
	const used = new Set<string>();
	return labels.map((label) => {
		let key = label.replace(/[^A-Za-z0-9_]+/g, '_').replace(/^_+|_+$/g, '') || 'Field';
		if (!/^[A-Za-z_]/.test(key)) key = `_${key}`;
		// draw.io keeps these on the cell itself.
		if (['id', 'label', 'placeholders', 'tooltip', 'link'].includes(key)) key = `${key}_`;
		let unique = key;
		for (let n = 2; used.has(unique); n++) unique = `${key}_${n}`;
		used.add(unique);
		return unique;
	});
}
