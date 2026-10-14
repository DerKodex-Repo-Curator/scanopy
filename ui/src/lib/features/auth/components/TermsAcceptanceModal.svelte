<script lang="ts">
	import { FileText, LogOut } from 'lucide-svelte';
	import GenericModal from '$lib/shared/components/layout/GenericModal.svelte';
	import { useAcceptTermsMutation, useLogoutMutation } from '$lib/features/auth/queries';
	import {
		auth_termsAccept,
		auth_termsUpdatedBody,
		auth_termsUpdatedTitle,
		common_logout
	} from '$lib/paraglide/messages';

	// No onClose: the server refuses this user's requests until they accept, so
	// the only ways out are accepting and logging out.
	let { isOpen }: { isOpen: boolean } = $props();

	const acceptTermsMutation = useAcceptTermsMutation();
	const logoutMutation = useLogoutMutation();
	let busy = $derived(acceptTermsMutation.isPending || logoutMutation.isPending);
</script>

<GenericModal
	title={auth_termsUpdatedTitle()}
	{isOpen}
	size="md"
	preventCloseOnClickOutside={true}
	showCloseButton={false}
>
	{#snippet headerIcon()}
		<FileText class="h-5 w-5 text-blue-400" />
	{/snippet}
	<div class="px-6 py-4">
		<!-- eslint-disable-next-line svelte/no-at-html-tags -- static i18n string with fixed links -->
		<p class="text-secondary text-sm">{@html auth_termsUpdatedBody()}</p>
	</div>
	{#snippet footer()}
		<div class="modal-footer flex justify-end gap-2">
			<button
				type="button"
				class="btn-secondary flex items-center gap-1.5 text-sm"
				disabled={busy}
				onclick={() => logoutMutation.mutate()}
			>
				<LogOut size={14} />
				{common_logout()}
			</button>
			<button
				type="button"
				class="btn-primary text-sm"
				disabled={busy}
				onclick={() => acceptTermsMutation.mutate()}
			>
				{auth_termsAccept()}
			</button>
		</div>
	{/snippet}
</GenericModal>
