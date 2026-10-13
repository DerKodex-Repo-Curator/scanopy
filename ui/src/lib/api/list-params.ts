import { createQuerySerializer } from 'openapi-fetch';

const serializeQuery = createQuerySerializer();

/**
 * A query serializer that sends a list's params as written, after the typed query.
 *
 * A list's view is already in its endpoint's param names and values (`DataControls`'
 * `onQueryChange`), and the same params are its URL. Sending them through untouched keeps the
 * request byte-for-byte what the URL describes, rather than parsing them back into typed options
 * only to serialize them again.
 */
export function withListParams(list: URLSearchParams | undefined) {
	return (query: Record<string, unknown>): string =>
		[serializeQuery(query), list?.toString() ?? ''].filter(Boolean).join('&');
}
