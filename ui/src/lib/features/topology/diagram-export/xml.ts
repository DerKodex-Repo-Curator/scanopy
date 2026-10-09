/**
 * Escape text for an XML attribute value or text node. Also drops characters XML 1.0 forbids,
 * which a device-supplied name can contain.
 */
export function escapeXml(value: string): string {
	return (
		value
			// eslint-disable-next-line no-control-regex
			.replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F￾￿]/g, '')
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
