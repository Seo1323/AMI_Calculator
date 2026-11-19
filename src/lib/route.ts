// Route helpers. Dependency-free so `node --test` can run them directly.

/**
 * The root page has an empty pathname when the app is served from `tauri://localhost` (Linux and
 * macOS), while Windows (`http://tauri.localhost/`), the dev server and a browser give "/".
 */
export const normalizePath = (pathname: string): string => pathname || '/';

export const isRoot = (pathname: string): boolean => normalizePath(pathname) === '/';

/** Whether the nav entry `href` is the current page (the root entry only on the root page). */
export const isActivePath = (href: string, pathname: string): boolean =>
	href === '/' ? isRoot(pathname) : normalizePath(pathname).startsWith(href);
