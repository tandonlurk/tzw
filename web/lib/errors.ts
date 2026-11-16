/**
 * Hint shown when the backend cannot be reached. A browser reports a CORS / Origin refusal (the API answers 403
 * origin_not_allowed) the same way as a dead server, so name both causes and the page origin to compare with CORS_ORIGIN.
 */
export function unreachableHint(pageOrigin: string | null, apiUrl: string): string {
  const origin = pageOrigin ? ` This page was opened from ${pageOrigin}; the backend only accepts the origin set in its CORS_ORIGIN (for example http://localhost:3000), so open the app at that exact address.` : "";
  return `Either the backend is not running at ${apiUrl}, or it refused this page's origin.${origin}`;
}
