import { describe, it, expect } from 'vitest';
import {
	assignActionKeys,
	DELETE_KEY,
	RESERVED_ACTION_KEYS
} from '$lib/shared/components/data/controls/actionShortcuts';
import type { CardAction } from '$lib/shared/components/data/types';
import type { IconComponent } from '$lib/shared/utils/types';

const action = (label: string, extra: Partial<CardAction> = {}): CardAction => ({
	label,
	icon: null as unknown as IconComponent,
	onClick: () => {},
	...extra
});

const keys = (actions: CardAction[]) => assignActionKeys(actions).map((a) => a.key);

describe('assignActionKeys', () => {
	it('takes each label’s first free letter, skipping the list’s own keys', () => {
		expect(keys([action('Edit'), action('Rescan'), action('Consolidate')])).toEqual([
			'E',
			'R',
			'O'
		]);
		expect(RESERVED_ACTION_KEYS).toContain('C');
	});

	it('gives a destructive action Delete, whatever its label', () => {
		expect(keys([action('Edit'), action('Delete', { class: 'btn-icon-danger' })])).toEqual([
			'E',
			DELETE_KEY
		]);
	});

	it('moves to a later letter when an earlier action took the first', () => {
		expect(keys([action('Edit'), action('Enable')])).toEqual(['E', 'N']);
	});

	it('leaves an action keyless when every letter is taken', () => {
		expect(keys([action('Tc')])).toEqual([null]);
	});
});
