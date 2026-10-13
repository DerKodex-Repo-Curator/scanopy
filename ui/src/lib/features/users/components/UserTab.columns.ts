import type { FieldConfig, LabelledCardFieldItem } from '$lib/shared/components/data/types';
import { siteItems } from '$lib/features/sites/columns';
import { permissions, entities } from '$lib/shared/stores/metadata';
import type { Site } from '$lib/features/sites/types';
import type { OrganizationInvite } from '$lib/features/organizations/types';
import { isUser, type User, type UserOrInvite } from '../types';
import {
	common_all,
	common_sites,
	common_email,
	common_emailAndPassword,
	common_none,
	common_expires,
	common_joined,
	common_role,
	common_status,
	common_unknownEntity,
	common_updated,
	common_url,
	common_user,
	common_you,
	invites_createdBy,
	invites_pendingInvite,
	users_authMethod,
	users_authMethodOidcAndPassword,
	users_emailVerified,
	users_providerLinked,
	users_termsAccepted
} from '$lib/paraglide/messages';

/** What the user columns read from the tab, each as a getter so values stay live. */
export interface UserColumnsContext {
	currentUser: () => User | null | undefined;
	users: () => User[];
	sites: () => Site[];
	/** The invite's accept link. */
	inviteUrl: (invite: OrganizationInvite) => string;
}

/** The site filter value of an Admin or Owner, who sees every site. */
const ALL_SITES = 'all';

/** The password half of an auth-method filter value; the provider's name is the other half. */
const PASSWORD_METHOD = 'password';
const NO_METHOD = 'none';

/**
 * How a user signs in, as a filter value: `<provider>`, `<provider>+password`, `password` or
 * `none`. Provider names come from the server's OIDC config, so they read the same in any language.
 */
function authMethodValue(item: UserOrInvite): string | null {
	if (!isUser(item)) return null;
	const { oidc_provider, has_password } = item.data;
	if (oidc_provider) return has_password ? `${oidc_provider}+${PASSWORD_METHOD}` : oidc_provider;
	return has_password ? PASSWORD_METHOD : NO_METHOD;
}

function authMethodLabel(value: string): string {
	if (value === PASSWORD_METHOD) return common_emailAndPassword();
	if (value === NO_METHOD) return common_none();
	const suffix = `+${PASSWORD_METHOD}`;
	return value.endsWith(suffix)
		? users_authMethodOidcAndPassword({ provider: value.slice(0, -suffix.length) })
		: value;
}

/**
 * The sites a user can reach.
 *
 * Admins and owners reach every site, which is a different statement from
 * being assigned all of them — so it reads as one "All" chip rather than a
 * list that would go stale the moment a site is added.
 */
function userSiteItems(ctx: UserColumnsContext, item: UserOrInvite): LabelledCardFieldItem[] {
	if (!isUser(item)) return [];
	const user = item.data;

	if (user.permissions === 'Admin' || user.permissions === 'Owner') {
		return [{ id: ALL_SITES, label: common_all(), color: entities.getColorHelper('Site').color }];
	}

	return siteItems(user.site_ids ?? [], ctx.sites());
}

/**
 * The list holds two shapes, so every field resolves for both. A field that
 * only one variant has returns empty for the other.
 */
export function userColumns(ctx: UserColumnsContext): FieldConfig<UserOrInvite>[] {
	return [
		{
			key: 'email',
			label: common_email(),
			type: 'string',
			searchable: true,
			sortable: true,
			getValue(item) {
				return isUser(item) ? item.data.email : item.data.send_to || invites_pendingInvite();
			},
			display: { primary: true, width: 220 }
		},
		{
			key: 'invite_status',
			label: common_status(),
			type: 'string',
			filterable: true,
			groupable: true,
			sortable: true,
			getValue: (item) =>
				!isUser(item)
					? invites_pendingInvite()
					: item.data.id === ctx.currentUser()?.id
						? common_you()
						: common_user(),
			getFilterValue: (item) =>
				!isUser(item) ? 'pending' : item.data.id === ctx.currentUser()?.id ? 'you' : 'user',
			filterLabel: (value) =>
				value === 'pending'
					? invites_pendingInvite()
					: value === 'you'
						? common_you()
						: common_user(),
			display: {
				getItems: (item) => {
					if (!isUser(item)) {
						return [{ id: 'pending', label: invites_pendingInvite(), color: 'Yellow' }];
					}
					return item.data.id === ctx.currentUser()?.id
						? [{ id: 'you', label: common_you(), color: 'Yellow' }]
						: [];
				}
			}
		},
		{
			key: 'permissions',
			label: common_role(),
			type: 'string',
			searchable: true,
			filterable: true,
			groupable: true,
			sortable: true,
			// Every role the backend defines.
			filterOptions: permissions.getItems().map((role) => role.id),
			filterLabel: (role) => permissions.getName(role) || role,
			getFilterValue: (item) => item.data.permissions,
			getValue(item) {
				return permissions.getName(item.data.permissions) || item.data.permissions;
			},
			display: {
				getItems: (item) => {
					const role = item.data.permissions;
					return [
						{
							id: role,
							label: permissions.getName(role) || role,
							color: permissions.getColorHelper(role).color
						}
					];
				}
			}
		},
		{
			key: 'site_ids',
			label: common_sites(),
			type: 'array',
			searchable: true,
			// Users share sites, so this filters; as an array it neither sorts nor groups.
			filterable: true,
			getValue: (item) => userSiteItems(ctx, item).map((n) => n.label),
			getFilterValue: (item) => userSiteItems(ctx, item).map((n) => n.id),
			filterLabel: (id) =>
				id === ALL_SITES ? common_all() : (ctx.sites().find((n) => n.id === id)?.name ?? id),
			display: { getItems: (item) => userSiteItems(ctx, item) }
		},
		{
			key: 'oidc_provider',
			label: users_authMethod(),
			type: 'string',
			searchable: true,
			filterable: true,
			groupable: true,
			sortable: true,
			getValue: (item) => {
				const value = authMethodValue(item);
				return value === null ? null : authMethodLabel(value);
			},
			getFilterValue: authMethodValue,
			filterLabel: authMethodLabel
		},
		{
			key: 'created_at',
			label: common_joined(),
			type: 'date',
			sortable: true,
			getValue: (item) => item.data.created_at
		},
		{
			key: 'invite_url',
			label: common_url(),
			type: 'string',
			searchable: true,
			getValue: (item) => (isUser(item) ? '' : ctx.inviteUrl(item.data)),
			display: { hiddenByDefault: true }
		},
		{
			key: 'invited_by',
			label: invites_createdBy(),
			type: 'string',
			searchable: true,
			sortable: true,
			groupable: true,
			filterable: true,
			getValue: (item) =>
				isUser(item)
					? null
					: ctx.users().find((u) => u.id == item.data.created_by)?.email ||
						common_unknownEntity({ entity: common_user() }),
			getFilterValue: (item) => (isUser(item) ? null : item.data.created_by),
			filterLabel: (id) =>
				ctx.users().find((u) => u.id === id)?.email ??
				common_unknownEntity({ entity: common_user() }),
			display: { hiddenByDefault: true }
		},
		{
			key: 'expires_at',
			label: common_expires(),
			type: 'date',
			sortable: true,
			getValue: (item) => (isUser(item) ? null : item.data.expires_at),
			display: { hiddenByDefault: true }
		},
		{
			key: 'updated_at',
			label: common_updated(),
			type: 'date',
			sortable: true,
			getValue: (item) => item.data.updated_at,
			display: { hiddenByDefault: true }
		},
		// Account facts an invite does not have yet, so its cells stay empty.
		{
			key: 'email_verified',
			label: users_emailVerified(),
			type: 'boolean',
			filterable: true,
			groupable: true,
			getValue: (item) => (isUser(item) ? (item.data.email_verified ?? null) : null),
			display: { hiddenByDefault: true }
		},
		{
			key: 'oidc_linked_at',
			label: users_providerLinked(),
			type: 'date',
			sortable: true,
			getValue: (item) => (isUser(item) ? (item.data.oidc_linked_at ?? null) : null),
			display: { hiddenByDefault: true }
		},
		{
			key: 'terms_accepted_at',
			label: users_termsAccepted(),
			type: 'date',
			sortable: true,
			getValue: (item) => (isUser(item) ? (item.data.terms_accepted_at ?? null) : null),
			display: { hiddenByDefault: true }
		}
	];
}
