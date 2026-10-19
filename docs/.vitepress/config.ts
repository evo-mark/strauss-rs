import type { HeadConfig, TransformContext, UserConfigFn } from "vitepress";

type PageData = TransformContext["pageData"];

const SITE_URL = "https://evo-mark.github.io/strauss-rs";
const SITE_NAME = "Strauss-RS";
const SITE_DESCRIPTION =
  "A Rust port of Strauss, which prefixes the namespaces of a Composer project's dependencies";
const REPOSITORY_URL = "https://github.com/evo-mark/strauss-rs";

/**
 * The absolute URL of a page, from its path relative to the docs folder.
 * `guide/why.md` is `<site>/guide/why.html` and `index.md` is `<site>/`.
 */
function pageUrl(relativePath: string): string {
  const path = relativePath
    .replace(/\.md$/, ".html")
    .replace(/(^|\/)index\.html$/, "$1");

  return `${SITE_URL}/${path}`;
}

/** An image given in frontmatter, as an absolute URL. */
function imageUrl(image: string): string {
  return /^https?:\/\//.test(image)
    ? image
    : `${SITE_URL}/${image.replace(/^\//, "")}`;
}

/** JSON for a script tag, safe to put in HTML. */
function jsonLd(data: Record<string, unknown>): string {
  return JSON.stringify({ "@context": "https://schema.org", ...data }).replace(
    /</g,
    "\\u003c",
  );
}

/**
 * The SEO tags for one page, read from its frontmatter and what VitePress
 * works out from the page (its title is the first heading).
 *
 * Frontmatter a page can set:
 *
 *   title        The page title. Otherwise the first heading.
 *   description  The meta description, and the one used for sharing. Otherwise
 *                the site's.
 *   image        A picture for link previews, as a path in `public/` or a full
 *                URL. A PNG or JPEG: most sites ignore SVG. Without one there
 *                is no preview picture.
 *   imageAlt     What the picture shows.
 *   canonical    The canonical URL, if it isn't the page's own.
 *   noindex      `true` to keep the page out of search results.
 *   type         Open Graph type: `article` (the default) or `website`.
 */
function seoTags(pageData: PageData): HeadConfig[] {
  const frontmatter = pageData.frontmatter ?? {};
  const isHome = frontmatter.layout === "home";

  // A page that is missing, or asks to be left out, is only marked as such.
  if (pageData.isNotFound || frontmatter.noindex === true) {
    return [["meta", { name: "robots", content: "noindex, nofollow" }]];
  }

  const title: string = pageData.title || SITE_NAME;
  const fullTitle =
    isHome || title === SITE_NAME ? title : `${title} | ${SITE_NAME}`;
  const description: string = pageData.description || SITE_DESCRIPTION;
  const url: string = frontmatter.canonical ?? pageUrl(pageData.relativePath);
  const image: string | undefined = frontmatter.image
    ? imageUrl(frontmatter.image)
    : undefined;

  const tags: HeadConfig[] = [
    ["link", { rel: "canonical", href: url }],

    ["meta", { property: "og:site_name", content: SITE_NAME }],
    ["meta", { property: "og:locale", content: "en_GB" }],
    [
      "meta",
      {
        property: "og:type",
        content: frontmatter.type ?? (isHome ? "website" : "article"),
      },
    ],
    ["meta", { property: "og:title", content: fullTitle }],
    ["meta", { property: "og:description", content: description }],
    ["meta", { property: "og:url", content: url }],

    [
      "meta",
      {
        name: "twitter:card",
        content: image ? "summary_large_image" : "summary",
      },
    ],
    ["meta", { name: "twitter:title", content: fullTitle }],
    ["meta", { name: "twitter:description", content: description }],
  ];

  if (image) {
    tags.push(
      ["meta", { property: "og:image", content: image }],
      ["meta", { name: "twitter:image", content: image }],
    );

    if (frontmatter.imageAlt) {
      tags.push(
        ["meta", { property: "og:image:alt", content: frontmatter.imageAlt }],
        ["meta", { name: "twitter:image:alt", content: frontmatter.imageAlt }],
      );
    }
  }

  // What the page is, for search engines and AI tools that read structured data.
  const data = isHome
    ? {
        "@type": "SoftwareApplication",
        name: SITE_NAME,
        description,
        url,
        applicationCategory: "DeveloperApplication",
        operatingSystem: "Linux, macOS, Windows",
        license: "https://opensource.org/licenses/MIT",
        sameAs: [REPOSITORY_URL],
      }
    : {
        "@type": "TechArticle",
        headline: title,
        description,
        url,
        isPartOf: { "@type": "WebSite", name: SITE_NAME, url: `${SITE_URL}/` },
        // Only when `lastUpdated` is turned on in the config.
        ...(pageData.lastUpdated
          ? { dateModified: new Date(pageData.lastUpdated).toISOString() }
          : {}),
      };

  tags.push(["script", { type: "application/ld+json" }, jsonLd(data)]);

  return tags;
}

const config: UserConfigFn<unknown> = () => ({
  base: "/strauss-rs/",
  title: "Strauss-RS",
  description:
    "A Rust port of Strauss, which prefixes the namespaces of a Composer project's dependencies",
  head: [
    [
      "link",
      { rel: "icon", type: "image/svg+xml", href: "/strauss-rs/logo.svg" },
    ],
  ],
  transformHead({ pageData }) {
    return seoTags(pageData);
  },
  sitemap: {
    hostname: "https://evo-mark.github.io/strauss-rs/",
  },
  themeConfig: {
    logo: { light: "/logo.svg", dark: "/logo-light.svg" },
    siteTitle: false,

    nav: [
      { text: "Guide", link: "/guide/getting-started" },
      { text: "Configuration", link: "/reference/configuration" },
      { text: "Benchmarks", link: "/benchmarks" },
    ],

    sidebar: [
      {
        text: "Guide",
        items: [
          { text: "Why prefix?", link: "/guide/why" },
          { text: "Getting started", link: "/guide/getting-started" },
          { text: "Moving from Strauss PHP", link: "/guide/migrating" },
          { text: "Updating your own code", link: "/guide/updating-your-code" },
          { text: "The autoloader", link: "/guide/autoloader" },
          { text: "Leaving things alone", link: "/guide/exclusions" },
          { text: "Common problems", link: "/guide/common-problems" },
        ],
      },
      {
        text: "Reference",
        items: [
          { text: "Configuration", link: "/reference/configuration" },
          { text: "Command line", link: "/reference/cli" },
          { text: "Installer script", link: "/reference/installer" },
        ],
      },
      {
        text: "Other",
        items: [
          { text: "How it works", link: "/how-it-works" },
          { text: "Benchmarks", link: "/benchmarks" },
          { text: "Test coverage", link: "/test-coverage" },
        ],
      },
    ],

    socialLinks: [
      { icon: "github", link: "https://github.com/evo-mark/strauss-rs" },
    ],

    search: { provider: "local" },

    footer: {
      message:
        "Released under the MIT licence. Strauss-RS is based on Strauss by Brian Henry.",
      copyright: "Copyright evoMark",
    },
  },
});

// https://vitepress.dev/reference/site-config
export default config;
