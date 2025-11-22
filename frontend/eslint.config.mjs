import { dirname } from "path";
import { fileURLToPath } from "url";
import { FlatCompat } from "@eslint/eslintrc";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const compat = new FlatCompat({
  baseDirectory: __dirname,
});

const nextConfigs = compat.extends("next/core-web-vitals", "next/typescript").map((config) => ({
  ...config,
  settings: {
    ...(config.settings ?? {}),
    next: {
      ...(config.settings?.next ?? {}),
      rootDir: ["apps/landing", "apps/webapp"],
    },
  },
}));

const eslintConfig = [
  ...nextConfigs,
  {
    rules: {
      "@next/next/no-html-link-for-pages": "off",
    },
  },
  {
    files: ["packages/**/*.{ts,tsx}"],
    rules: {
      "@next/next/no-html-link-for-pages": "off",
    },
  },
  {
    ignores: [
      "node_modules/**",
      ".next/**",
      "out/**",
      "build/**",
      "**/dist/**",
      "**/.turbo/**",
      "**/next-env.d.ts",
    ],
  },
];

export default eslintConfig;
