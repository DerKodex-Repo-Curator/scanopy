<script lang="ts">
	import { formatLongDate } from '$lib/shared/utils/formatting';
	import CodeContainer from '$lib/shared/components/data/CodeContainer.svelte';
	import DocsHint from '$lib/shared/components/feedback/DocsHint.svelte';
	import InlineDanger from '$lib/shared/components/feedback/InlineDanger.svelte';
	import InlineInfo from '$lib/shared/components/feedback/InlineInfo.svelte';
	import InlineWarning from '$lib/shared/components/feedback/InlineWarning.svelte';
	import GenericModal from '$lib/shared/components/layout/GenericModal.svelte';
	import ModalHeaderIcon from '$lib/shared/components/layout/ModalHeaderIcon.svelte';
	import { entities } from '$lib/shared/stores/metadata';
	import { ArrowBigUpDash } from 'lucide-svelte';
	import type { Daemon, UpgradeStep } from '../types/base';
	import { VERSION } from '$lib/version';
	import { type DaemonOS, detectOS, isPreUnifiedDaemon } from '../utils';
	import { useDaemonUpgradeCommandQuery } from '../queries';
	import { trackEvent } from '$lib/shared/utils/analytics';
	import OsSelector from './OsSelector.svelte';
	import {
		common_close,
		common_stepNumber,
		daemons_currentVersion,
		daemons_dockerApplyChanges,
		daemons_dockerLatestTag,
		daemons_dockerLinuxOnly,
		daemons_dockerLinuxOnlyBody,
		daemons_dockerPinnedVersion,
		daemons_latestVersion,
		daemons_updateAvailable,
		daemons_upgradeConfigPreserved,
		daemons_upgradeDownload,
		daemons_upgradeDaemon,
		daemons_upgradeLegacyServiceName,
		daemons_upgradeRestartService,
		daemons_upgradeStartService,
		daemons_upgradeStopService,
		daemons_docsUpgradeDockerConfigLost,
		daemons_docsUpgradeDockerConfigLostLinkText,
		daemons_upgradeDockerConfigCopyStep,
		daemons_upgradeDockerConfigWarningBody,
		daemons_upgradeDockerConfigWarningTitle,
		daemons_sunsetDeprecatedTitle,
		daemons_sunsetDeprecatedBody,
		daemons_sunsetUnsupportedTitle,
		daemons_sunsetUnsupportedBody,
		discovery_upgradeConsolidationWarning
	} from '$lib/paraglide/messages';

	interface Props {
		isOpen?: boolean;
		onClose: () => void;
		daemon: Daemon;
	}

	let { isOpen = false, onClose, daemon }: Props = $props();

	// Server-published sunset for this daemon's version (set for Deprecated /
	// Unsupported). The UI renders the same date the sunset email uses; it never
	// computes its own notion of "deprecated".
	let sunsetDate = $derived(daemon.version_status.sunset_date ?? null);
	// Date-only value: render in UTC so it shows the published day in every zone.
	let sunsetDateDisplay = $derived(sunsetDate ? formatLongDate(sunsetDate, 'UTC') : '');
	let isUnsupported = $derived(daemon.version_status.status === 'Unsupported');
	let sunsetDaysRemaining = $derived.by(() => {
		if (!sunsetDate) return 0;
		const ms = new Date(`${sunsetDate}T00:00:00Z`).getTime() - Date.now();
		return Math.max(0, Math.ceil(ms / 86_400_000));
	});

	// OS selection: the daemon's recorded OS, reported by the daemon or picked at creation, which
	// also hides the OS picker; with none recorded the user picks, starting from the browser's OS.
	let recordedOS = $derived(daemon.os ?? null);
	let pickedOS = $state<DaemonOS | null>(null);
	let selectedOS = $derived<DaemonOS>(recordedOS ?? pickedOS ?? detectOS());

	type LinuxMethod = 'binary' | 'docker';
	let linuxMethod: LinuxMethod = $state('binary');

	// The server builds every upgrade command from the daemon record: download the new binary,
	// then restart the installed service, which the daemon resolves from its id.
	const upgradeQuery = useDaemonUpgradeCommandQuery(() => daemon.id);
	let artifacts = $derived(upgradeQuery.data ?? null);
	let binarySteps = $derived<UpgradeStep[]>(artifacts ? artifacts[selectedOS] : []);
	let shellLanguage = $derived(selectedOS === 'windows' ? 'powershell' : 'bash');

	const stepLabels: Record<UpgradeStep['type'], () => string> = {
		Download: () => daemons_upgradeDownload(),
		Restart: () => daemons_upgradeRestartService(),
		Stop: () => daemons_upgradeStopService(),
		Start: () => daemons_upgradeStartService()
	};

	function handleOsSelect(os: DaemonOS) {
		pickedOS = os;
		trackEvent('daemon_upgrade_os_selected', { os });
	}

	let colorHelper = entities.getColorHelper('Daemon');
</script>

<GenericModal {isOpen} title={daemons_upgradeDaemon()} size="xl" {onClose}>
	{#snippet headerIcon()}
		<ModalHeaderIcon Icon={ArrowBigUpDash} color={colorHelper.color} />
	{/snippet}

	<div class="flex min-h-0 flex-1 flex-col">
		<div class="flex-1 overflow-auto p-6">
			<div class="space-y-6">
				<p class="text-secondary">
					{daemons_updateAvailable()} <span class="text-primary font-medium">{daemon.name}</span>.
					{#if daemon.version_status.version}
						{daemons_currentVersion()}
						<span class="font-mono">{daemon.version_status.version}.</span>
					{/if}
					{daemons_latestVersion()} <span class="font-mono">{VERSION}.</span>
				</p>

				{#if sunsetDate && isUnsupported}
					<InlineDanger
						title={daemons_sunsetUnsupportedTitle()}
						body={daemons_sunsetUnsupportedBody({ date: sunsetDateDisplay })}
					/>
				{:else if sunsetDate}
					<InlineWarning
						title={daemons_sunsetDeprecatedTitle()}
						body={daemons_sunsetDeprecatedBody({
							date: sunsetDateDisplay,
							days: sunsetDaysRemaining
						})}
					/>
				{/if}

				<InlineInfo title="" body={daemons_upgradeConfigPreserved()} />

				{#if isPreUnifiedDaemon(daemon)}
					<InlineWarning
						title=""
						body={discovery_upgradeConsolidationWarning()}
						dismissableKey="unified-discovery-migration"
					/>
				{/if}

				<OsSelector
					{selectedOS}
					onOsSelect={handleOsSelect}
					showOs={!recordedOS}
					{linuxMethod}
					onLinuxMethodChange={(method) => (linuxMethod = method)}
				>
					{#if selectedOS === 'linux' && linuxMethod === 'docker'}
						<!-- Linux Docker Compose -->
						{#if artifacts}
							<div class="space-y-3">
								{#if artifacts.docker.config_copy}
									<InlineWarning
										title={daemons_upgradeDockerConfigWarningTitle()}
										body={daemons_upgradeDockerConfigWarningBody()}
									/>
									<div class="text-secondary">
										<b>{common_stepNumber({ number: '1' })}</b>
										{daemons_upgradeDockerConfigCopyStep()}
									</div>
									<CodeContainer
										language="bash"
										expandable={false}
										code={artifacts.docker.config_copy}
									/>
									<div class="text-secondary">
										<b>{common_stepNumber({ number: '2' })}</b>
										{daemons_dockerLatestTag()}
									</div>
									<CodeContainer language="bash" expandable={false} code={artifacts.docker.pull} />
									<DocsHint
										text={daemons_docsUpgradeDockerConfigLost()}
										href="https://scanopy.net/docs/self-hosted-server/troubleshooting/#daemon-waits-for-apiinitialize-after-a-restart-or-recreate"
										linkText={daemons_docsUpgradeDockerConfigLostLinkText()}
									/>
								{:else}
									<div class="space-y-2">
										<p class="text-secondary text-sm">
											{daemons_dockerLatestTag()}
										</p>
										<CodeContainer
											language="bash"
											expandable={false}
											code={artifacts.docker.pull}
										/>
									</div>
								{/if}

								<div class="space-y-2">
									<p class="text-secondary text-sm">
										{daemons_dockerPinnedVersion()}
										<span class="font-mono">docker-compose.yml</span>:
									</p>
									<CodeContainer
										language="yaml"
										expandable={false}
										code={artifacts.docker.pinned_image}
									/>
									<p class="text-secondary text-sm">
										{daemons_dockerApplyChanges()}
									</p>
								</div>
							</div>
						{/if}
					{:else if artifacts}
						<!-- Binary: the server's steps for this OS (download, then restart the service) -->
						<div class="space-y-3">
							{#each binarySteps as step, index (index)}
								<div class="text-secondary">
									<b>{common_stepNumber({ number: String(index + 1) })}</b>
									{stepLabels[step.type]()}
								</div>
								<CodeContainer language={shellLanguage} expandable={false} code={step.command} />
							{/each}

							{#if artifacts.legacy_service_name}
								<InlineInfo title="" body={daemons_upgradeLegacyServiceName()} />
							{/if}

							{#if selectedOS !== 'linux'}
								<InlineInfo
									title={daemons_dockerLinuxOnly()}
									body={daemons_dockerLinuxOnlyBody()}
								/>
							{/if}
						</div>
					{/if}
				</OsSelector>
			</div>
		</div>

		<!-- Footer -->
		<div class="modal-footer">
			<div class="flex items-center justify-end">
				<button type="button" class="btn-secondary" onclick={onClose}>{common_close()}</button>
			</div>
		</div>
	</div>
</GenericModal>
