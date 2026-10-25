function parse(url) {
  try {
    return new URL(url);
  } catch {
    return null;
  }
}

export function isSameOrigin(url, origin) {
  return parse(url)?.origin === origin;
}

export function isExternalHttpUrl(url) {
  const parsed = parse(url);
  return parsed?.protocol === 'http:' || parsed?.protocol === 'https:';
}

/** What to do with a window.open() request from a page on `origin`. */
export function windowOpenAction(url, origin) {
  if (origin && isSameOrigin(url, origin)) return 'in-app';
  if (isExternalHttpUrl(url)) return 'external';
  return 'deny';
}
