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
        background: 'var(--color-background)',
        surface: {
          DEFAULT: 'var(--color-surface)',
          raised: 'var(--color-surface-raised)',
        },
        border: {
          DEFAULT: 'var(--color-border)',
          strong: 'var(--color-border-strong)',
        },
        text: {
          primary: 'var(--color-text-primary)',
          secondary: 'var(--color-text-secondary)',
          muted: 'var(--color-text-muted)',
          inverse: 'var(--color-text-inverse)',
        },
        accent: {
          DEFAULT: 'var(--color-accent)',
          hover: 'var(--color-accent-hover)',
          muted: 'var(--color-accent-muted)',
          foreground: 'var(--color-accent-foreground)',
        },
        bark: {
          50: 'var(--color-bark-50)',
          100: 'var(--color-bark-100)',
          200: 'var(--color-bark-200)',
          300: 'var(--color-bark-300)',
          400: 'var(--color-bark-400)',
          500: 'var(--color-bark-500)',
          600: 'var(--color-bark-600)',
          700: 'var(--color-bark-700)',
          800: 'var(--color-bark-800)',
          900: 'var(--color-bark-900)',
          950: 'var(--color-bark-950)',
        },
        moss: {
          50: 'var(--color-moss-50)',
          100: 'var(--color-moss-100)',
          200: 'var(--color-moss-200)',
          300: 'var(--color-moss-300)',
          400: 'var(--color-moss-400)',
          500: 'var(--color-moss-500)',
          600: 'var(--color-moss-600)',
          700: 'var(--color-moss-700)',
          800: 'var(--color-moss-800)',
          900: 'var(--color-moss-900)',
          950: 'var(--color-moss-950)',
        },
        paper: {
          50: 'var(--color-paper-50)',
          100: 'var(--color-paper-100)',
          200: 'var(--color-paper-200)',
          300: 'var(--color-paper-300)',
        },
        danger: {
          DEFAULT: 'var(--color-danger)',
          muted: 'var(--color-danger-muted)',
        },
        warning: {
          DEFAULT: 'var(--color-warning)',
          muted: 'var(--color-warning-muted)',
        },
        info: {
          DEFAULT: 'var(--color-info)',
          muted: 'var(--color-info-muted)',
        },
        success: {
          DEFAULT: 'var(--color-success)',
          muted: 'var(--color-success-muted)',
        },
        column: {
          backlog: {
            bg: 'var(--column-backlog-bg)',
            border: 'var(--column-backlog-border)',
            accent: 'var(--column-backlog-accent)',
          },
          ready: {
            bg: 'var(--column-ready-bg)',
            border: 'var(--column-ready-border)',
            accent: 'var(--column-ready-accent)',
          },
          'in-progress': {
            bg: 'var(--column-in-progress-bg)',
            border: 'var(--column-in-progress-border)',
            accent: 'var(--column-in-progress-accent)',
          },
          'in-review': {
            bg: 'var(--column-in-review-bg)',
            border: 'var(--column-in-review-border)',
            accent: 'var(--column-in-review-accent)',
          },
          'in-qa': {
            bg: 'var(--column-in-qa-bg)',
            border: 'var(--column-in-qa-border)',
            accent: 'var(--column-in-qa-accent)',
          },
          'wait-final': {
            bg: 'var(--column-wait-final-bg)',
            border: 'var(--column-wait-final-border)',
            accent: 'var(--column-wait-final-accent)',
          },
          done: {
            bg: 'var(--column-done-bg)',
            border: 'var(--column-done-border)',
            accent: 'var(--column-done-accent)',
          },
          blocked: {
            bg: 'var(--column-blocked-bg)',
            border: 'var(--column-blocked-border)',
            accent: 'var(--column-blocked-accent)',
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
    },
  },
  plugins: [],
}
