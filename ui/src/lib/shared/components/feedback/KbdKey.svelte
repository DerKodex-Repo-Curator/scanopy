<script lang="ts">
	interface Props {
		key: string;
		size?: 'xs' | 'sm' | 'md' | 'lg';
		/**
		 * `inherit` takes the surrounding text colour, for a key shown inside a button: an outline
		 * in the button's text colour on no fill, sized to sit within the button's line.
		 */
		tone?: 'default' | 'inherit';
		class?: string;
	}

	let { key, size = 'md', tone = 'default', class: className = '' }: Props = $props();

	const sizeClasses: Record<string, string> = {
		xs: 'min-w-4 h-4 px-1 text-[10px] leading-none',
		sm: 'min-w-5 h-5 px-1 text-[10px]',
		md: 'min-w-6 h-6 px-1.5 text-xs',
		lg: 'min-w-8 h-8 px-2 text-sm'
	};

	const toneClasses: Record<string, string> = {
		default:
			'border-gray-300 bg-gray-50 text-gray-600 shadow-[0_1px_0_1px_rgba(0,0,0,0.08)] dark:border-gray-600 dark:bg-gray-700 dark:text-gray-300 dark:shadow-[0_1px_0_1px_rgba(0,0,0,0.3)]',
		inherit:
			'bg-transparent text-current [border-color:color-mix(in_srgb,currentColor_45%,transparent)]'
	};
</script>

<kbd
	class="inline-flex items-center justify-center rounded border font-mono font-medium {toneClasses[
		tone
	]} {sizeClasses[size]} {className}"
>
	{key}
</kbd>
