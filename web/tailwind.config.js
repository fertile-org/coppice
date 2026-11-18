import animate from 'tailwindcss-animate'

/** Enable Tailwind `<token>/<n>` opacity on CSS-variable colors (Tailwind 3.4). */
function withAlpha(varName) {
  return `color-mix(in srgb, var(${varName}) calc(<alpha-value> * 100%), transparent)`
}

/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  theme: {
    extend: {
      fontFamily: {
        display: ['var(--font-display)'],
        body: ['var(--font-body)'],
        mono: ['var(--font-mono)'],
      },
      colors: {
        background: withAlpha('--color-background'),
        surface: {
          DEFAULT: withAlpha('--color-surface'),
          raised: withAlpha('--color-surface-raised'),
        },
        field: {
          DEFAULT: withAlpha('--color-field-bg'),
          disabled: withAlpha('--color-field-bg-disabled'),
        },
        border: {
          DEFAULT: withAlpha('--color-border'),
          strong: withAlpha('--color-border-strong'),
        },
        'switch-track-off': withAlpha('--color-switch-track-off'),
        overlay: {
          DEFAULT: withAlpha('--color-overlay'),
          strong: withAlpha('--color-overlay-strong'),
        },
        text: {
          primary: withAlpha('--color-text-primary'),
          secondary: withAlpha('--color-text-secondary'),
          muted: withAlpha('--color-text-muted'),
          inverse: withAlpha('--color-text-inverse'),
        },
        accent: {
          DEFAULT: withAlpha('--color-accent'),
          hover: withAlpha('--color-accent-hover'),
          muted: withAlpha('--color-accent-muted'),
          foreground: withAlpha('--color-accent-foreground'),
        },
        btn: {
          primary: {
            DEFAULT: withAlpha('--color-btn-primary'),
            hover: withAlpha('--color-btn-primary-hover'),
            fg: withAlpha('--color-btn-primary-fg'),
          },
          secondary: {
            DEFAULT: withAlpha('--color-btn-secondary'),
            hover: withAlpha('--color-btn-secondary-hover'),
            border: withAlpha('--color-btn-secondary-border'),
            fg: withAlpha('--color-btn-secondary-fg'),
          },
          disabled: {
            border: withAlpha('--color-btn-disabled-border'),
            fg: withAlpha('--color-btn-disabled-fg'),
          },
        },
        bark: {
          50: withAlpha('--color-bark-50'),
          100: withAlpha('--color-bark-100'),
          200: withAlpha('--color-bark-200'),
          300: withAlpha('--color-bark-300'),
          400: withAlpha('--color-bark-400'),
          500: withAlpha('--color-bark-500'),
          600: withAlpha('--color-bark-600'),
          700: withAlpha('--color-bark-700'),
          800: withAlpha('--color-bark-800'),
          900: withAlpha('--color-bark-900'),
          950: withAlpha('--color-bark-950'),
        },
        moss: {
          50: withAlpha('--color-moss-50'),
          100: withAlpha('--color-moss-100'),
          200: withAlpha('--color-moss-200'),
          300: withAlpha('--color-moss-300'),
          400: withAlpha('--color-moss-400'),
          500: withAlpha('--color-moss-500'),
          600: withAlpha('--color-moss-600'),
          700: withAlpha('--color-moss-700'),
          800: withAlpha('--color-moss-800'),
          900: withAlpha('--color-moss-900'),
          950: withAlpha('--color-moss-950'),
        },
        paper: {
          50: withAlpha('--color-paper-50'),
          100: withAlpha('--color-paper-100'),
          200: withAlpha('--color-paper-200'),
          300: withAlpha('--color-paper-300'),
        },
        danger: {
          DEFAULT: withAlpha('--color-danger'),
          muted: withAlpha('--color-danger-muted'),
        },
        warning: {
          DEFAULT: withAlpha('--color-warning'),
          muted: withAlpha('--color-warning-muted'),
        },
        info: {
          DEFAULT: withAlpha('--color-info'),
          muted: withAlpha('--color-info-muted'),
        },
        success: {
          DEFAULT: withAlpha('--color-success'),
          muted: withAlpha('--color-success-muted'),
        },
        column: {
          backlog: {
            bg: withAlpha('--column-backlog-bg'),
            border: withAlpha('--column-backlog-border'),
            accent: withAlpha('--column-backlog-accent'),
          },
          ready: {
            bg: withAlpha('--column-ready-bg'),
            border: withAlpha('--column-ready-border'),
            accent: withAlpha('--column-ready-accent'),
          },
          'plan-review': {
            bg: withAlpha('--column-plan-review-bg'),
            border: withAlpha('--column-plan-review-border'),
            accent: withAlpha('--column-plan-review-accent'),
          },
          'in-progress': {
            bg: withAlpha('--column-in-progress-bg'),
            border: withAlpha('--column-in-progress-border'),
            accent: withAlpha('--column-in-progress-accent'),
          },
          'in-review': {
            bg: withAlpha('--column-in-review-bg'),
            border: withAlpha('--column-in-review-border'),
            accent: withAlpha('--column-in-review-accent'),
          },
          'in-qa': {
            bg: withAlpha('--column-in-qa-bg'),
            border: withAlpha('--column-in-qa-border'),
            accent: withAlpha('--column-in-qa-accent'),
          },
          'wait-final': {
            bg: withAlpha('--column-wait-final-bg'),
            border: withAlpha('--column-wait-final-border'),
            accent: withAlpha('--column-wait-final-accent'),
          },
          done: {
            bg: withAlpha('--column-done-bg'),
            border: withAlpha('--column-done-border'),
            accent: withAlpha('--column-done-accent'),
          },
        },
      },
      spacing: {
        0: 'var(--space-0)',
        1: 'var(--space-1)',
        2: 'var(--space-2)',
        3: 'var(--space-3)',
        4: 'var(--space-4)',
        5: 'var(--space-5)',
        6: 'var(--space-6)',
        8: 'var(--space-8)',
        10: 'var(--space-10)',
        12: 'var(--space-12)',
        16: 'var(--space-16)',
      },
      borderRadius: {
        sm: 'var(--radius-sm)',
        md: 'var(--radius-md)',
        lg: 'var(--radius-lg)',
        xl: 'var(--radius-xl)',
        full: 'var(--radius-full)',
      },
      boxShadow: {
        sm: 'var(--shadow-sm)',
        md: 'var(--shadow-md)',
        lg: 'var(--shadow-lg)',
        card: 'var(--shadow-card)',
      },
      transitionDuration: {
        fast: 'var(--duration-fast)',
        normal: 'var(--duration-normal)',
        slow: 'var(--duration-slow)',
      },
      transitionTimingFunction: {
        out: 'var(--ease-out)',
        'in-out': 'var(--ease-in-out)',
      },
      keyframes: {
        'fade-in': {
          '0%': { opacity: '0' },
          '100%': { opacity: '1' },
        },
        'toast-progress': {
          '0%': { width: '100%' },
          '100%': { width: '0%' },
        },
        'overlay-in': {
          from: { opacity: '0' },
          to: { opacity: '1' },
        },
        'overlay-out': {
          from: { opacity: '1' },
          to: { opacity: '0' },
        },
        // Opacity + Y translate only — no scale (softens text on macOS Retina).
        'dialog-in': {
          from: { opacity: '0', translate: '0 0.5rem' },
          to: { opacity: '1', translate: '0 0' },
        },
        'dialog-out': {
          from: { opacity: '1', translate: '0 0' },
          to: { opacity: '0', translate: '0 0.25rem' },
        },
        'sheet-in': {
          from: { opacity: '0' },
          to: { opacity: '1' },
        },
        'sheet-out': {
          from: { opacity: '1' },
          to: { opacity: '0' },
        },
      },
      animation: {
        'fade-in': 'fade-in 200ms var(--ease-out) forwards',
        'toast-progress': 'toast-progress linear forwards',
        'overlay-in': 'overlay-in 150ms var(--ease-out)',
        'overlay-out': 'overlay-out 120ms ease-in forwards',
        'dialog-in': 'dialog-in 190ms var(--ease-out)',
        'dialog-out': 'dialog-out 130ms ease-in forwards',
        'sheet-in': 'sheet-in 150ms var(--ease-out)',
        'sheet-out': 'sheet-out 120ms ease-in forwards',
      },
    },
  },
  plugins: [animate],
}
