/**
 * Tracks whether a value has changed since it was last observed, treating the
 * first observation as "no change".
 *
 * Three effects need this shape: ordering, search, and the tag filter each have
 * to tell the parent when the user changed something, and each has to stay
 * quiet on the run that merely restores saved state. Firing on that first run
 * would reset the restored page to 1 on every mount, so the skip is load-bearing
 * rather than an optimisation.
 *
 * The comparison is supplied because the values are not all primitives — a tag
 * filter is a list, where identity says nothing about equality.
 */
export interface ChangeTracker<V> {
	/**
	 * Record `value`, returning whether it differs from the previously recorded
	 * one. Always false on the first call, which only establishes the baseline.
	 */
	changed(value: V): boolean;
}

export function createChangeTracker<V>(
	isEqual: (a: V, b: V) => boolean = Object.is
): ChangeTracker<V> {
	let previous: V;
	let initialized = false;

	return {
		changed(value: V): boolean {
			if (!initialized) {
				previous = value;
				initialized = true;
				return false;
			}

			if (isEqual(previous, value)) return false;

			previous = value;
			return true;
		}
	};
}
