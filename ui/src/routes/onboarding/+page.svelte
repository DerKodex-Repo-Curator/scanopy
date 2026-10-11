<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import { themeStore } from '$lib/shared/stores/theme.svelte';
	import { ChevronLeft } from 'lucide-svelte';
	import Toast from '$lib/shared/components/feedback/Toast.svelte';
	import OrgSitesModal from '$lib/features/auth/components/onboarding/OrgSitesModal.svelte';
	import OrgLimitModal from '$lib/features/auth/components/OrgLimitModal.svelte';
	import RegisterModal from '$lib/features/auth/components/RegisterModal.svelte';
	import UseCaseStep from '$lib/features/auth/components/onboarding/UseCaseStep.svelte';
	import HostingStep from '$lib/features/auth/components/onboarding/HostingStep.svelte';
	import type { RegisterRequest, SetupRequest, UseCase } from '$lib/features/auth/types/base';
	import {
		useSetupMutation,
		useRegisterMutation,
		useOnboardingStepMutation,
		useOnboardingStateQuery
	} from '$lib/features/auth/queries';
	import { fetchOrganization } from '$lib/features/organizations/queries';
	import { navigate } from '$lib/shared/utils/navigation';
	import { resolve } from '$app/paths';
	import {
		asksForSite,
		onboardingStore,
		onboardingSteps,
		type OnboardingStep
	} from '$lib/features/auth/stores/onboarding';
	import { trackEvent } from '$lib/shared/utils/analytics';
	import { pushError } from '$lib/shared/stores/feedback';
	import {
		auth_emailAlreadyInUse,
		common_goBack,
		onboarding_noCreditCard,
		onboarding_stepProgress
	} from '$lib/paraglide/messages';
	import {
		useConfigQuery,
		isCloud,
		isLicenseSigningAvailable
	} from '$lib/shared/stores/config-query';
	import { setLastLoginMethod } from '$lib/features/auth/components/LoginModal.svelte';

	// Show OIDC error from redirect if present
	onMount(() => {
		// `?hosting=self_hosted` preselects the hosting step and opens the
		// post-registration plan picker on the Self-Hosted tab. Persisted so it
		// survives registration and OIDC redirects.
		const hosting = $page.url.searchParams.get('hosting');
		if (hosting === 'self_hosted' || hosting === 'cloud') {
			onboardingStore.setHosting(hosting);
		}

		const error = $page.url.searchParams.get('error');
		const errorCode = $page.url.searchParams.get('error_code');
		if (error) {
			// If the error indicates a duplicate email, redirect to login
			if (errorCode === 'user_email_in_use') {
				pushError(auth_emailAlreadyInUse());
				setTimeout(() => goto(resolve('/login')), 1500);
			} else {
				pushError(error);
			}
			const url = new URL(window.location.href);
			url.searchParams.delete('error');
			url.searchParams.delete('error_code');
			window.history.replaceState({}, '', url.toString());
		}
	});

	// TanStack Query mutations and queries
	const setupMutation = useSetupMutation();
	const registerMutation = useRegisterMutation();
	const onboardingStepMutation = useOnboardingStepMutation();
	const onboardingStateQuery = useOnboardingStateQuery();

	const onboardingConfigQuery = useConfigQuery();
	let onboardingConfigData = $derived(onboardingConfigQuery.data);

	// URL params for invite flow
	let orgName = $derived($page.url.searchParams.get('org_name'));
	let invitedBy = $derived($page.url.searchParams.get('invited_by'));

	// Determine if this is an invite flow (skip to register)
	let isInviteFlow = $derived(!!invitedBy);

	// Block the whole new-org flow (before use-case selection) when the instance
	// is at its org cap. Invited users join an existing org, so they are exempt.
	let showOrgLimitGate = $derived((onboardingConfigData?.org_limit_reached ?? false) && !invitedBy);
	let adminContactEmail = $derived(onboardingConfigData?.server_admin_contact_email ?? null);

	// Step tracking
	type Step = OnboardingStep;

	// A server that sells both cloud and self-hosted plans asks where Scanopy runs.
	let asksHosting = $derived(
		!!onboardingConfigData &&
			isCloud(onboardingConfigData) &&
			isLicenseSigningAvailable(onboardingConfigData)
	);
	let steps = $derived(onboardingSteps(asksHosting, $onboardingStore.hosting));

	// Get initial step from URL params or default
	function getInitialStep(): Step {
		if ($page.url.searchParams.get('invited_by')) return 'register';
		return 'hosting';
	}

	let requestedStep = $state<Step>(getInitialStep());
	// A step this flow doesn't have (hosting on a server that doesn't ask it, or
	// a use case restored for a self-hosted buyer) falls back to the first step.
	let currentStep = $derived(
		isInviteFlow || steps.includes(requestedStep) ? requestedStep : steps[0]
	);
	let stepInitialized = $state(false);
	let lastPersistedStep = $state<Step | null>(null);

	// Restore step and store data from session on mount
	$effect(() => {
		if (!stepInitialized && onboardingStateQuery.data && !isInviteFlow) {
			const stateData = onboardingStateQuery.data;

			// Restore step
			if (stateData.step && isValidStep(stateData.step)) {
				requestedStep = stateData.step;
				lastPersistedStep = stateData.step; // Don't re-persist this
			}

			// Restore use case
			if (stateData.use_case && isValidUseCase(stateData.use_case)) {
				onboardingStore.setUseCase(stateData.use_case as UseCase);
			}

			// Restore org name
			if (stateData.org_name) {
				onboardingStore.setOrganizationName(stateData.org_name);
			}

			// Restore site (with ID and name)
			if (stateData.site) {
				onboardingStore.setSite({
					id: stateData.site.id ?? undefined,
					name: stateData.site.name
				});
			}

			stepInitialized = true;
		}
	});

	// Helper to validate use case
	function isValidUseCase(useCase: string): useCase is UseCase {
		return ['homelab', 'internal_it', 'msp'].includes(useCase);
	}

	// Helper to validate step
	function isValidStep(step: string): step is Step {
		return ['hosting', 'use_case', 'setup', 'register'].includes(step);
	}

	// The step list depends on the server config, so steps are neither shown nor
	// persisted until it loads.
	let configLoaded = $derived(!!onboardingConfigData);

	// A self-hosted license buyer on cloud: this account holds the license, not a
	// cloud org, so it skips the site and the cloud-app copy.
	let licenseSignup = $derived(
		!asksForSite($onboardingStore.hosting, !!onboardingConfigData && isCloud(onboardingConfigData))
	);

	// Persist step to session whenever it changes
	$effect(() => {
		if (stepInitialized && configLoaded && !isInviteFlow && currentStep !== lastPersistedStep) {
			lastPersistedStep = currentStep;
			onboardingStepMutation.mutate({
				step: currentStep,
				use_case: useCase ?? undefined
			});
		}
	});

	// Get use case from store
	let useCase = $derived($onboardingStore.useCase);

	// Invite: just register = 1 step
	let totalSteps = $derived(isInviteFlow ? 1 : steps.length);
	let currentStepNumber = $derived(isInviteFlow ? 1 : steps.indexOf(currentStep) + 1);

	// Note: Auth check is handled by +layout.svelte

	function goToNextStep() {
		const next = steps[steps.indexOf(currentStep) + 1];
		if (next) requestedStep = next;
	}

	async function handleSetupSubmit(formData: SetupRequest) {
		try {
			// Submit setup data to backend (stored in session)
			const result = await setupMutation.mutateAsync(formData);
			// Update store with site ID
			if (result.site_id) onboardingStore.setSiteId(result.site_id);

			// Track onboarding modal completion
			trackEvent('onboarding_modal_completed', {
				site_count: formData.site ? 1 : 0
			});

			requestedStep = 'register';
		} catch {
			// Error handled by mutation
		}
	}

	function handleBack() {
		const previous = steps[steps.indexOf(currentStep) - 1];
		if (previous) requestedStep = previous;
	}

	async function handleRegister(data: RegisterRequest) {
		try {
			await registerMutation.mutateAsync(data);
			setLastLoginMethod('email');

			// Before clearing onboarding store, get state for tracking
			const state = onboardingStore.getState();

			// Track successful registration with context
			trackEvent('onboarding_registration_completed', {
				use_case: state.useCase
			});

			// Fetch organization data before navigating
			await fetchOrganization();

			// Clear onboarding store
			onboardingStore.reset();

			// Navigate to main app — daemon prompt auto-opens via +page.svelte
			// for new orgs without daemons (both cloud and non-cloud)
			await navigate();
		} catch {
			// Error handled by mutation
		}
	}

	function handleSwitchToLogin() {
		goto(resolve('/login'));
	}

	function handleClose() {
		// Don't allow closing during onboarding
	}
</script>

<div class="relative flex min-h-screen flex-col items-center bg-[var(--color-bg-elevated)] p-4">
	<!-- Background image with overlay -->
	<div class="absolute inset-0 z-0">
		<div
			class="h-full w-full bg-cover bg-center bg-no-repeat blur-[2px]"
			style="background-image: url('/images/background-{themeStore.resolvedTheme}.webp')"
		></div>
		<div
			class="absolute inset-0 {themeStore.resolvedTheme === 'dark' ? 'bg-black/30' : 'bg-white/15'}"
		></div>
	</div>

	<!-- Progress Indicator - fixed position above modal (hidden for invite flow
	     and when the org-limit gate replaces the whole flow) -->
	{#if !isInviteFlow && !showOrgLimitGate && configLoaded}
		<div class="fixed left-1/2 top-2 z-[200] -translate-x-1/2 sm:top-6">
			<div class="flex flex-col items-center gap-1">
				<div
					class="flex items-center gap-2 rounded-full bg-white/90 px-4 py-2 shadow-lg backdrop-blur-sm dark:bg-gray-800/90"
				>
					{#if currentStepNumber > 1}
						<button
							type="button"
							onclick={handleBack}
							class="text-secondary hover:text-primary -ml-1 flex items-center transition-colors"
							aria-label={common_goBack()}
						>
							<ChevronLeft class="h-4 w-4" />
						</button>
					{/if}
					<span class="text-secondary text-sm">
						{onboarding_stepProgress({ current: currentStepNumber, total: totalSteps })}
					</span>
					<div class="flex gap-1">
						<!-- eslint-disable-next-line @typescript-eslint/no-unused-vars -->
						{#each Array(totalSteps) as _, i (i)}
							<div
								class="h-2 w-2 rounded-full transition-colors {i < currentStepNumber
									? 'bg-primary-500'
									: 'bg-gray-300 dark:bg-gray-600'}"
							></div>
						{/each}
					</div>
				</div>
				{#if onboardingConfigData && isCloud(onboardingConfigData)}
					<span class="text-tertiary text-xs">{onboarding_noCreditCard()}</span>
				{/if}
			</div>
		</div>
	{/if}

	<!-- Content container -->
	<div class="flex flex-1 items-center justify-center pt-8 sm:pt-0">
		<div class="relative z-10 w-full">
			{#if showOrgLimitGate}
				<!-- Instance at its org cap: block the new-org flow up front -->
				<OrgLimitModal
					{adminContactEmail}
					onClose={handleClose}
					onSwitchToLogin={handleSwitchToLogin}
				/>
			{:else if !isInviteFlow && !configLoaded}
				<!-- The step list waits on the server config -->
			{:else if currentStep === 'hosting'}
				<HostingStep
					isOpen={true}
					onNext={goToNextStep}
					onClose={handleClose}
					onSwitchToLogin={handleSwitchToLogin}
				/>
			{:else if currentStep === 'use_case'}
				<!-- Use Case Selection Step -->
				<UseCaseStep
					isOpen={true}
					onNext={goToNextStep}
					onClose={handleClose}
					onSwitchToLogin={handleSwitchToLogin}
				/>
			{:else if currentStep === 'setup'}
				<!-- Organization & Site Setup -->
				<OrgSitesModal
					isOpen={true}
					onClose={handleClose}
					onSubmit={handleSetupSubmit}
					{useCase}
					{licenseSignup}
				/>
			{:else if currentStep === 'register'}
				<!-- Registration -->
				<RegisterModal
					isOpen={true}
					onRegister={handleRegister}
					onClose={handleClose}
					onSwitchToLogin={handleSwitchToLogin}
					{orgName}
					{invitedBy}
				/>
			{/if}
		</div>
	</div>

	<Toast />
</div>
