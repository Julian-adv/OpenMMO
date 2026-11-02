import js from '@eslint/js'
import globals from 'globals'
import tseslint from 'typescript-eslint'
import sveltePlugin from 'eslint-plugin-svelte'

export default [
  {
    ignores: ['dist/', 'public/', '.svelte-kit/', '.vite/', 'src/lib/wasm/'],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...sveltePlugin.configs['flat/recommended'],
  {
    files: ['**/*.{js,mjs,cjs,ts,mts,cts,svelte}'],
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node,
      },
    },
    rules: {
      '@typescript-eslint/no-unused-vars': [
        'error',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
        },
      ],
    },
  },
  {
    files: ['**/*.svelte'],
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
      },
    },
    rules: {
      // TS type-checking (npm run check) already covers this; no-undef
      // misfires on ambient global types like GoogleCredentialResponse.
      'no-undef': 'off',
    },
  },
]
