/** Caption and alt handling for blog markdown. Other pages are left alone. */

function filePath(file) {
  const value = file?.path || file?.history?.[0] || '';
  return String(value).replace(/\\/g, '/');
}

export function isBlogMarkdown(file) {
  return filePath(file).includes('/content/blog/');
}

function walk(node, visit) {
  if (!node || typeof node !== 'object') return;
  visit(node);
  for (const child of node.children ?? []) walk(child, visit);
}

export function blogHeadingErrors(tree) {
  const errors = [];
  walk(tree, (node) => {
    if (node.type === 'heading' && node.depth === 1) {
      errors.push('Blog posts use the title field as the only h1. Start the body at a second-level heading.');
    }
  });
  return errors;
}

function meaningful(children) {
  return (children ?? []).filter((child) => !(child.type === 'text' && !String(child.value ?? '').trim()));
}

function altOf(img) {
  const alt = img.properties?.alt;
  return typeof alt === 'string' ? alt.trim() : '';
}

function titleOf(img) {
  const title = img.properties?.title;
  return typeof title === 'string' ? title.trim() : '';
}

function figureFor(img, caption) {
  img.properties ??= {};
  delete img.properties.title;
  if (!img.properties.loading) img.properties.loading = 'lazy';
  return {
    type: 'element',
    tagName: 'figure',
    properties: { className: ['blog-figure'] },
    children: [
      img,
      {
        type: 'element',
        tagName: 'figcaption',
        properties: {},
        children: [{ type: 'text', value: caption }],
      },
    ],
  };
}

function prepareImage(img, errors) {
  img.properties ??= {};
  if (!altOf(img)) errors.push('blog image needs alt text');
  if (!img.properties.loading) img.properties.loading = 'lazy';
  const caption = titleOf(img);
  return caption ? figureFor(img, caption) : null;
}

/** Mutates a hast tree. Images need alt. A markdown title becomes the caption. */
export function transformBlogImages(tree) {
  const errors = [];
  const visit = (node) => {
    if (!node || typeof node !== 'object' || !Array.isArray(node.children)) return;
    for (let index = 0; index < node.children.length; index += 1) {
      const child = node.children[index];
      if (child?.type === 'element' && child.tagName === 'p') {
        const bits = meaningful(child.children);
        if (bits.length === 1 && bits[0]?.type === 'element' && bits[0].tagName === 'img') {
          const figure = prepareImage(bits[0], errors);
          if (figure) node.children[index] = figure;
          continue;
        }
      }
      if (child?.type === 'element' && child.tagName === 'img') {
        const figure = prepareImage(child, errors);
        if (figure) node.children[index] = figure;
        continue;
      }
      visit(child);
    }
  };
  visit(tree);
  return errors;
}

export function remarkBlogHeadings() {
  return (tree, file) => {
    if (!isBlogMarkdown(file)) return;
    const errors = blogHeadingErrors(tree);
    if (errors.length) file.fail(errors.join('\n'));
  };
}

export function rehypeBlogFigures() {
  return (tree, file) => {
    if (!isBlogMarkdown(file)) return;
    const errors = transformBlogImages(tree);
    if (errors.length) file.fail(errors.join('\n'));
  };
}
