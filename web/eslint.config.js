import js from '@eslint/js'
import globals from 'globals'
import reactHooks from 'eslint-plugin-react-hooks'
import reactRefresh from 'eslint-plugin-react-refresh'
import tseslint from 'typescript-eslint'
import { defineConfig, globalIgnores } from 'eslint/config'

export default defineConfig([
  globalIgnores(['dist']),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      globals: globals.browser,
    },
    rules: {
      'no-restricted-globals': [
        'error',
        {
          name: 'confirm',
          message: 'Use useConfirm() instead of the native confirm dialog.',
        },
        {
          name: 'alert',
          message: 'Use an in-app dialog or toast instead of alert().',
        },
        {
          name: 'prompt',
          message: 'Use an in-app dialog instead of prompt().',
        },
      ],
      'no-restricted-properties': [
        'error',
        {
          object: 'window',
          property: 'confirm',
          message: 'Use useConfirm() instead of window.confirm.',
        },
        {
          object: 'window',
          property: 'alert',
          message: 'Use an in-app dialog or toast instead of window.alert.',
        },
        {
          object: 'window',
          property: 'prompt',
          message: 'Use an in-app dialog instead of window.prompt.',
        },
      ],
    },
  },
])
