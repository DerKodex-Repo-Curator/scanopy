import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { get } from 'svelte/store';
import { closeModal, initModalFromUrl, modalState } from '$lib/shared/stores/modal-registry';
import { openCredentialCreate, typesForIntegration } from '$lib/features/credentials/utils/prefill';
import type { CredentialTypeMetadata } from '$lib/shared/stores/metadata';

const type = (
	id: string,
	integration: CredentialTypeMetadata['integration'],
	targets: CredentialTypeMetadata['targets']
) => ({ id, metadata: { fields: [], integration, targets } });

describe('typesForIntegration', () => {
	const types = [
		type('DockerProxy', 'Docker', ['DaemonHost', 'Hosts']),
		type('DockerSocket', 'Docker', ['DaemonHost']),
		type('UnifiApiKey', 'UnifiController', ['DaemonHost', 'Hosts']),
		type('UnifiLocalAdmin', 'UnifiController', ['DaemonHost', 'Hosts'])
	];

	it('offers only transports a host can be assigned to when hosts come pre-assigned', () => {
		expect(typesForIntegration(types, 'Docker', true)).toEqual(['DockerProxy']);
		expect(typesForIntegration(types, 'Docker', false)).toEqual(['DockerProxy', 'DockerSocket']);
	});

	it('keeps every transport of an integration with several, in fixture order', () => {
		expect(typesForIntegration(types, 'UnifiController', true)).toEqual([
			'UnifiApiKey',
			'UnifiLocalAdmin'
		]);
	});
});

describe('openCredentialCreate', () => {
	let href = 'http://localhost/#home';
	beforeEach(() => {
		vi.stubGlobal('window', {
			get location() {
				const url = new URL(href);
				return {
					href,
					search: url.search,
					get hash() {
						return url.hash;
					},
					set hash(value: string) {
						url.hash = value;
						href = url.toString();
					}
				};
			},
			history: {
				replaceState: (_: unknown, __: string, next: string) => {
					href = next;
				}
			}
		});
	});
	afterEach(() => {
		closeModal();
		vi.unstubAllGlobals();
		href = 'http://localhost/#home';
	});

	// The digest email links the same URL, so a reload or an emailed link must open the same form.
	it('survives the URL: a reload opens the same pre-filled create flow', () => {
		openCredentialCreate('Proxmox', ['h1', 'h2']);
		const link = href;

		// A fresh page load at that URL.
		closeModal();
		href = link;
		initModalFromUrl();

		expect(get(modalState)).toMatchObject({
			name: 'credential-editor',
			id: null,
			prefill: { integration: 'Proxmox', hosts: 'h1,h2' }
		});
		expect(new URL(link).hash).toBe('#credentials');
	});
});
