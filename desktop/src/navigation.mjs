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
