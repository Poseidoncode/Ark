export function getRepoName(path: string): string {
  if (!path || path.trim() === "") return "";
  if (path.includes("\0") || path.includes("\n") || path.includes("\r")) return "";

  const cleanPath = path.replace(/[/\\]+$/, '');

  if (cleanPath === "" || cleanPath === "." || cleanPath === "..") return "";

  if (cleanPath.endsWith('.git')) {
    const withoutGit = cleanPath.slice(0, -4).replace(/[/\\]+$/, '');
    const parts = withoutGit.split(/[/\\]/).filter(Boolean);
    return parts.length > 0 ? parts[parts.length - 1] : "";
  }

  const parts = cleanPath.split(/[/\\]/).filter(Boolean);
  if (parts.length === 0) return "";
  const lastPart = parts[parts.length - 1];

  if (!lastPart || lastPart === '.' || lastPart === '..') {
    return parts.length >= 2 ? parts[parts.length - 2] : "";
  }

  return lastPart;
}
