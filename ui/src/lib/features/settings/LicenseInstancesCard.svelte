<script lang="ts">
	import { Trash2 } from 'lucide-svelte';
	import InfoCard from '$lib/shared/components/data/InfoCard.svelte';
	import InlineWarning from '$lib/shared/components/feedback/InlineWarning.svelte';
	import ConfirmationDialog from '$lib/shared/components/feedback/ConfirmationDialog.svelte';
	import {
		useDeleteLicenseInstanceMutation,
		useLicenseInstancesQuery
	} from '$lib/features/billing/queries';
	import { formatTimestamp } from '$lib/shared/utils/formatting';
	import {
		common_delete,
		common_inactive,
		common_ipAddress,
		common_readOnly,
		common_servers,
		settings_billing_license_lastCheckIn,
		settings_billing_license_noServers,
		settings_billing_license_olderServers,
		settings_billing_license_overLimit,
		settings_billing_license_serverDeleteConfirm,
		settings_billing_license_serverDeleteTitle,
		settings_billing_license_serversHint
	} from '$lib/paraglide/messages';

	let { enabled }: { enabled: boolean } = $props();

	const instancesQuery = useLicenseInstancesQuery(() => enabled);
	const deleteMutation = useDeleteLicenseInstanceMutation();

	let instances = $derived(instancesQuery.data?.instances ?? []);
	let usage = $derived(instancesQuery.data?.usage ?? null);
	let pendingDelete = $state<string | null>(null);

	async function handleDelete() {
		const id = pendingDelete;
		pendingDelete = null;
		if (!id) return;
		try {
			await deleteMutation.mutateAsync(id);
		} catch {
			// The API client toasts the error.
		}
	}
</script>

<InfoCard title={common_servers()}>
	<div class="space-y-3">
		<p class="text-secondary text-sm">{settings_billing_license_serversHint()}</p>
		{#if usage && usage.limit != null && usage.active > usage.limit}
			<InlineWarning
				title={settings_billing_license_overLimit({ active: usage.active, limit: usage.limit })}
			/>
		{/if}
		{#if instancesQuery.isSuccess && instances.length === 0}
			<p class="text-secondary text-sm">{settings_billing_license_noServers()}</p>
		{/if}
		{#each instances as { instance, active, holds_slot } (instance.id)}
			<InfoCard variant="compact">
				<div class="flex items-center justify-between gap-4">
					<div class="min-w-0 {active ? '' : 'opacity-60'}">
						<p class="text-primary truncate text-sm font-medium">
							{#if instance.instance_id}
								<span class="font-mono">{instance.instance_id}</span>
							{:else}
								{settings_billing_license_olderServers()}
							{/if}
							{#if !active}
								<span class="text-secondary ml-2 text-xs font-normal">{common_inactive()}</span>
							{:else if !holds_slot}
								<span class="text-danger ml-2 text-xs font-normal">{common_readOnly()}</span>
							{/if}
						</p>
						<p class="text-secondary text-xs">
							{#if instance.last_seen_ip}
								{common_ipAddress()}: <span class="font-mono">{instance.last_seen_ip}</span> ·
							{/if}
							{#if instance.server_version}
								v{instance.server_version} ·
							{/if}
							{settings_billing_license_lastCheckIn()}: {formatTimestamp(instance.last_seen_at)}
						</p>
					</div>
					<button
						type="button"
						class="btn-secondary flex flex-shrink-0 items-center gap-2"
						onclick={() => (pendingDelete = instance.id)}
						disabled={deleteMutation.isPending}
					>
						<Trash2 class="h-4 w-4" />
						{common_delete()}
					</button>
				</div>
			</InfoCard>
		{/each}
	</div>
</InfoCard>

<ConfirmationDialog
	isOpen={pendingDelete != null}
	title={settings_billing_license_serverDeleteTitle()}
	message={settings_billing_license_serverDeleteConfirm()}
	confirmLabel={common_delete()}
	variant="danger"
	onConfirm={handleDelete}
	onCancel={() => (pendingDelete = null)}
	onClose={() => (pendingDelete = null)}
/>
