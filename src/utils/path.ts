export function getRepoName(path: string): string {
  if (!path || path.trim() === "") return "";

  const cleanPath = path.replace(/[/\\]+$/, '');

  if (cleanPath.endsWith('.git')) {
    const withoutGit = cleanPath.slice(0, -4).replace(/[/\\]+$/, '');
    const parts = withoutGit.split(/[/\\]/);
    return parts[parts.length - 1] || "";
  }

  const parts = cleanPath.split(/[/\\]/);
  const lastPart = parts[parts.length - 1];

  if (!lastPart || lastPart === '.' || lastPart === '..') {
    return parts[parts.length - 2] || path;
  }

  return lastPart || path;
}
