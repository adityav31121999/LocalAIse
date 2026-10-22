import { renderToStaticMarkup } from "react-dom/server";
import { createElement } from "react";
import { Markdown } from "../src/util/markdown";

/** Manual regression check for the chat markdown renderer.
 *
 * Run with: npx tsx scripts/check-markdown.tsx
 * Exits non-zero when any expected structure is missing from the output.
 */

const SAMPLE = `Here are a few quick questions that will help me tailor the budget plan:

1. **What's your main goal?**

(e.g., pay off debt, save for a vacation, build an emergency fund)

2. **What's your typical monthly income?**

- Draft the **monthly budget** — file_name stays untouched
- Show a table:

| Category | Amount |
| --- | ---: |
| Rent | $1,200 |

\`\`\`ts
const budget = { rent: 1200 };
\`\`\`

> quoted note
Visit https://example.com/docs or see the [docs](https://example.com).

## Heading two
Trailing **open`;

const EXPECT: Array<[string, RegExp]> = [
  ["bold parsed", /<strong>What&#x27;s your main goal\?<\/strong><\/li><\/ol>/],
  ["continuation paragraph", /<p>\(e\.g\., pay off debt/],
  ["ol restart numbering", /<ol start="2">/],
  ["table parsed", /<th>Category<\/th>[\s\S]*<td>\$1,200<\/td>/],
  ["fence parsed", /<pre><code class="lang-ts">const budget/],
  ["blockquote parsed", /<blockquote><p>quoted note<\/p><\/blockquote>/],
  ["safe link parsed", /<a href="https:\/\/example\.com"/],
  ["heading parsed", /<h2>Heading two<\/h2><p>Trailing/],
  // Mid-stream: unclosed ** stays literal until the closing token arrives.
  ["streaming text survives", /Trailing \*\*open<\/p><\/div>$/],
];

const html = renderToStaticMarkup(createElement(Markdown, { text: SAMPLE }));

let failed = 0;
for (const [label, pattern] of EXPECT) {
  if (!pattern.test(html)) {
    console.error(`❌ ${label} did not render`);
    failed += 1;
  } else {
    console.log(`✅ ${label}`);
  }
}

// Untrusted HTML must stay escaped — no raw markup passthrough ever.
const hostile = "<img src=x onerror=alert(1)> **hi**";
const hostileHtml = renderToStaticMarkup(createElement(Markdown, { text: hostile }));
if (hostileHtml.includes("<img")) {
  console.error("❌ raw HTML leaked into the DOM");
  failed += 1;
} else {
  console.log("✅ raw HTML is inert (rendered as text)");
}

if (failed > 0) {
  console.error("\n--- actual output ---\n" + html);
  process.exit(1);
}
console.log("\n✅ Markdown renderer verified.");
