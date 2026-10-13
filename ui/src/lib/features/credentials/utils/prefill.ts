import type { components } from '$lib/api/schema';
import type { CredentialTypeMetadata, TypedTypeMetadata } from '$lib/shared/stores/metadata';
import { openModal } from '$lib/shared/stores/modal-registry';

type CredentialIntegration = components['schemas']['CredentialIntegration'];

/**
 * Open the credential create flow for an integration, with these hosts assigned. The run report,
 * the home nudge and the digest email all land here, the email through the same URL params.
 */
export function openCredentialCreate(integration: CredentialIntegration, hostIds: string[]): void {
	window.location.hash = 'credentials';
	openModal('credential-editor', {
		prefill: { integration, hosts: hostIds.length ? hostIds.join(',') : undefined }
	});
}

/**
 * The credential types the create flow offers for a pre-filled integration: its transports, and
 * only those that can be assigned to a host when hosts come pre-assigned (a Docker socket reaches
 * only the daemon's own host). Fixture order, so the first is the one pre-selected.
 */
export function typesForIntegration(
	types: Pick<TypedTypeMetadata<CredentialTypeMetadata>, 'id' | 'metadata'>[],
	integration: string,
	forHosts: boolean
): string[] {
	return types
		.filter((t) => t.metadata?.integration === integration)
		.filter((t) => !forHosts || (t.metadata?.targets ?? []).includes('Hosts'))
		.map((t) => t.id);
}
