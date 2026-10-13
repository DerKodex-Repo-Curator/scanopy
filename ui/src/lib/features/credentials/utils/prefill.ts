import type { components } from '$lib/api/schema';
import type { CredentialTypeMetadata, TypedTypeMetadata } from '$lib/shared/stores/metadata';
import { openModal } from '$lib/shared/stores/modal-registry';
import { slugifySiteName } from '$lib/features/daemons/utils';

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

/**
 * The name a pre-filled credential starts with, snake_cased: the host's own name when it is for
 * one host, the integration's when it is for several (one host's name would mislabel the rest) or
 * the host has no name of its own.
 */
export function prefillCredentialName(
	hosts: { display_name?: string | null }[],
	integrationName: string
): string {
	const hostName = hosts.length === 1 ? hosts[0].display_name?.trim() : undefined;
	return slugifySiteName(hostName || integrationName, '_');
}
