export type StartupProject =
  | {
      readonly root: string;
      readonly projectFile: string;
    }
  | {
      readonly diagnostic: string;
    };

const MAX_ROOT_LENGTH = 4096;
const MAX_PROJECT_FILE_LENGTH = 1024;

export function readStartupProject(href: string): StartupProject | null {
  let url: URL;
  try {
    url = new URL(href, "http://127.0.0.1/");
  } catch {
    return { diagnostic: "Studio startup URL is malformed." };
  }
  const roots = url.searchParams.getAll("root");
  const files = url.searchParams.getAll("project");
  if (roots.length === 0 && files.length === 0) return null;
  if (roots.length !== 1 || files.length !== 1) {
    return {
      diagnostic:
        "Startup requires exactly one external root and one project-relative file.",
    };
  }
  const root = roots[0]?.trim() ?? "";
  const projectFile = files[0]?.trim() ?? "";
  if (
    root.length === 0 ||
    root.length > MAX_ROOT_LENGTH ||
    projectFile.length === 0 ||
    projectFile.length > MAX_PROJECT_FILE_LENGTH ||
    root.includes("\0") ||
    projectFile.includes("\0")
  ) {
    return {
      diagnostic: "Startup project selection is empty or malformed.",
    };
  }
  return { root, projectFile };
}
