<script lang="ts">
	// Help text for an action the server refuses until the user verifies their email. The
	// control it sits beside stays enabled: verifying from the emailed link and retrying works
	// without closing the surrounding modal. Renders nothing once verification isn't pending.
	import InlineWarning from '$lib/shared/components/feedback/InlineWarning.svelte';
	import {
		useEmailVerificationPending,
		useResendVerificationMutation
	} from '$lib/features/auth/queries';
	import { common_resend, common_sending, verifyEmail_title } from '$lib/paraglide/messages';

	let { body }: { body: string } = $props();

	const verification = useEmailVerificationPending();
	const resendMutation = useResendVerificationMutation();

	function handleResend() {
		if (verification.email) resendMutation.mutate({ email: verification.email });
	}
</script>

{#if verification.pending}
	<div class="space-y-1">
		<InlineWarning title={verifyEmail_title()} {body} />
		<button
			type="button"
			class="text-secondary text-xs underline hover:no-underline disabled:opacity-50"
			onclick={handleResend}
			disabled={resendMutation.isPending}
		>
			{resendMutation.isPending ? common_sending() : common_resend()}
		</button>
	</div>
{/if}
