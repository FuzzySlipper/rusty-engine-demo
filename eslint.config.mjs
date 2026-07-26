import nx from "@nx/eslint-plugin";

const depConstraints = [
  {
    sourceTag: "type:app",
    onlyDependOnLibsWithTags: ["type:feature", "type:lib"],
  },
  {
    sourceTag: "type:feature",
    onlyDependOnLibsWithTags: ["type:lib"],
  },
  {
    sourceTag: "type:lib",
    onlyDependOnLibsWithTags: ["type:lib"],
  },
  {
    sourceTag: "scope:platform",
    onlyDependOnLibsWithTags: [],
  },
  {
    sourceTag: "scope:theme",
    onlyDependOnLibsWithTags: [],
  },
  {
    sourceTag: "scope:components",
    onlyDependOnLibsWithTags: ["scope:theme"],
  },
  {
    sourceTag: "scope:feature",
    onlyDependOnLibsWithTags: [
      "scope:platform",
      "scope:components",
      "scope:theme",
    ],
  },
  {
    sourceTag: "scope:shell",
    onlyDependOnLibsWithTags: [
      "scope:feature",
      "scope:platform",
      "scope:components",
      "scope:theme",
    ],
  },
];

export default [
  ...nx.configs["flat/base"],
  ...nx.configs["flat/typescript"],
  ...nx.configs["flat/javascript"],
  {
    ignores: [
      "**/dist/**",
      "**/coverage/**",
      "**/node_modules/**",
      "target/**",
      "tmp/**",
    ],
  },
  {
    files: ["apps/**/*.ts", "libs/**/*.ts"],
    rules: {
      "@nx/enforce-module-boundaries": [
        "error",
        {
          allow: ["^.*/eslint(\\.base)?\\.config\\.[cm]?[jt]s$"],
          depConstraints,
          enforceBuildableLibDependency: false,
        },
      ],
      "@typescript-eslint/consistent-type-imports": "error",
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/no-non-null-assertion": "error",
    },
  },
];
