import type { Credential, CredentialOrderField } from '../types/base';
import { entityRef, type CardFieldItem } from '$lib/shared/components/data/types';
import { defineRegistryFields } from '$lib/shared/entity-fields/table';
import type { EntityLookup } from '$lib/shared/entity-fields';
import type { Site } from '$lib/features/sites/types';
import type { Tag } from '$lib/features/tags/types/base';
import { credentialTypes, entities } from '$lib/shared/stores/metadata';
import {
	getCredentialTypeId,
	getStabilityTagProps,
	getTargetTagProps,
	getUpstreamSupportTagProps
} from '$lib/features/credentials/types/base';
import type { Host } from '$lib/features/hosts/types/base';
import { hostDisplayName } from '$lib/features/hosts/host-display-name';
import osFamilies from '$lib/data/os-families.json';
import {
	common_beta,
	common_created,
	common_type,
	common_updated,
	credentials_unofficialApi,
	common_hosts,
	common_sites,
	common_tags,
	common_targets
} from '$lib/paraglide/messages';
import { tagNames } from '$lib/features/tags/columns';

/** What the credential columns read from the tab, each as a getter or function so values stay live. */
export interface CredentialColumnsContext {
	lookup: () => EntityLookup;
	tags: () => Tag[];
	sitesForCredential: (credential: Credential) => Site[];
	hostsForCredential: (credential: Credential) => Host[];
	targetsFor: (credential: Credential) => string[];
	daemonHostOnly: (credential: Credential) => boolean;
	assignmentItems: (applicable: boolean, items: CardFieldItem[]) => CardFieldItem[];
}

/**
 * Columns from the entity field registry; this adds how each one searches, filters and sorts.
 * Every CredentialOrderField value must be listed.
 */
export function credentialColumns(ctx: CredentialColumnsContext) {
	return defineRegistryFields<Credential, CredentialOrderField>(
		'Credential',
		ctx.lookup,
		{
			// Identity field: grouping by it would render a header per credential.
			name: {
				searchable: true,
				groupable: false,
				display: { primary: true, width: 220 }
			},
			created_at: { label: common_created(), type: 'date', display: { hiddenByDefault: true } },
			updated_at: { label: common_updated(), type: 'date', display: { hiddenByDefault: true } }
		},
		[
			{
				// Set only on credentials that read files or sockets on the daemon; empty otherwise.
				key: 'daemon_os',
				filterable: true,
				groupable: true,
				sortable: true,
				// The field holds an OS family, so the options are every family, named the same way.
				filterOptions: osFamilies.map((family) => family.name),
				getValue: (item: Credential) =>
					item.daemon_os
						? (osFamilies.find((family) => family.id === item.daemon_os)?.name ?? item.daemon_os)
						: null,
				display: { hiddenByDefault: true }
			},
			{
				key: 'credential_type',
				label: common_type(),
				type: 'string',
				searchable: true,
				filterable: true,
				// Type is the axis credentials are actually organized by.
				groupable: true,
				sortable: true,
				filterMode: 'include',
				filterOptions: credentialTypes.getItems().map((t) => credentialTypes.getName(t.id)),
				getValue: (item: Credential) => credentialTypes.getName(getCredentialTypeId(item)),
				display: {
					getItems: (item: Credential) => {
						const typeId = getCredentialTypeId(item);
						return [
							{
								id: typeId,
								...credentialTypes.getTag(typeId),
								icon: credentialTypes.getIconComponent(typeId)
							}
						];
					}
				}
			},
			{
				// Beta and unofficial API are properties of the type, each with its own column so
				// either can be grouped and filtered on its own. A cell shows the tag the wizard and
				// the type dropdown use, and stays empty for a stable or vendor-supported type.
				key: 'beta',
				label: common_beta(),
				type: 'boolean',
				filterable: true,
				groupable: true,
				getValue: (item: Credential) =>
					credentialTypes.getMetadata(getCredentialTypeId(item))?.stability === 'Beta',
				display: {
					getItems: (item: Credential) => {
						const typeId = getCredentialTypeId(item);
						const tag = getStabilityTagProps(credentialTypes.getMetadata(typeId)?.stability);
						return tag ? [{ id: `${typeId}-beta`, ...tag }] : [];
					}
				}
			},
			{
				key: 'unofficial_api',
				label: credentials_unofficialApi(),
				type: 'boolean',
				filterable: true,
				groupable: true,
				getValue: (item: Credential) =>
					credentialTypes.getMetadata(getCredentialTypeId(item))?.upstream_support ===
					'Undocumented',
				display: {
					getItems: (item: Credential) => {
						const typeId = getCredentialTypeId(item);
						const tag = getUpstreamSupportTagProps(
							credentialTypes.getMetadata(typeId)?.upstream_support
						);
						return tag ? [{ id: `${typeId}-unofficial-api`, ...tag }] : [];
					}
				}
			},
			{ key: 'description', searchable: true },
			{
				key: 'assigned_sites',
				label: common_sites(),
				type: 'array',
				searchable: true,
				// Credentials share sites, so this filters. Hosts are near-unique per credential,
				// so search covers those.
				filterable: true,
				getValue: (item: Credential) => ctx.sitesForCredential(item).map((n) => n.name),
				display: {
					getItems: (item: Credential) =>
						ctx.assignmentItems(
							ctx.targetsFor(item).includes('Site'),
							ctx.sitesForCredential(item).map((site) => ({
								id: site.id,
								label: site.name,
								color: entities.getColorHelper('Site').color,
								entityRef: entityRef('Site', site.id, site)
							}))
						)
				}
			},
			{
				key: 'assigned_hosts',
				label: common_hosts(),
				type: 'array',
				searchable: true,
				getValue: (item: Credential) => ctx.hostsForCredential(item).map(hostDisplayName),
				display: {
					getItems: (item: Credential) =>
						ctx.assignmentItems(
							// DaemonHost-only credentials (Docker/Podman sockets) are
							// assigned to their daemon's host through the same junction.
							ctx.targetsFor(item).some((t) => t === 'Hosts' || t === 'DaemonHost'),
							ctx.hostsForCredential(item).map((host) => ({
								id: host.id,
								label: hostDisplayName(host),
								// A daemon-host credential reaches that host *through* its
								// daemon, so it reads as a daemon relationship rather than a
								// host one. Only credentials that target hosts generally get
								// the host colour.
								color: entities.getColorHelper(ctx.daemonHostOnly(item) ? 'Daemon' : 'Host').color,
								entityRef: entityRef('Host', host.id, host)
							}))
						)
				}
			},
			{
				key: 'target',
				label: common_targets(),
				type: 'array',
				searchable: true,
				filterable: true,
				filterMode: 'include',
				// Every scope a credential type declares, read from the type metadata.
				filterOptions: [
					...new Set(
						credentialTypes
							.getItems()
							.flatMap((t) => credentialTypes.getMetadata(t.id).targets ?? [])
					)
				],
				getValue: (item: Credential) => {
					const typeId = getCredentialTypeId(item);
					const meta = credentialTypes.getMetadata(typeId);
					return meta?.targets ?? [];
				},
				display: {
					// Off by default: the target set is a property of the credential *type*, so it repeats
					// down the column for every credential of the same type and earns its width
					// only when someone is actually filtering by it. Still filterable. An array, so
					// it neither sorts nor groups.
					hiddenByDefault: true,
					// Coloured chips per target rather than undifferentiated grey.
					getItems: (item: Credential) => {
						const meta = credentialTypes.getMetadata(getCredentialTypeId(item));
						return (meta?.targets ?? []).map((target: string) => ({
							id: target,
							...getTargetTagProps(target)
						}));
					}
				}
			},
			{
				key: 'tags',
				label: common_tags(),
				type: 'array',
				searchable: true,
				filterable: true,
				getValue: (entity: Credential) => tagNames(entity.tags, ctx.tags())
			}
		]
	);
}
