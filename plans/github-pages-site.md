---
autonomy: auto
ci: wait
merge: manual
---

# GitHub Pages site

A one-page landing site for LotML under `site/`, published to GitHub Pages by a workflow. Its
layout follows plixlang.github.io and pipelex.com — dark hero, version pill, code window with
tabs, eyebrow-labelled feature grid, CLI and pipeline sections — in the lotus palette.

## Why

The README is the only front door, and it reads as a repository index rather than an
introduction. Done means `site/` presents the language alone — a real program, a tour of what
differs from Python, the agent-facing compiler and how to install it, with no research record,
papers or ADRs on the page; every LotML snippet on it passes `lotml check`; and a push to `main`
deploys it.

## Paths

- `site/` — `index.html`, `style.css`, `main.js`, the icon
- `.github/workflows/pages.yml`
- `harness/tests/test_site.py`
- `README.md`

## References

- `.github/assets/` — the lotus icon and banner the palette is taken from
- `reference/lotml.md` — the language every snippet is written in
- adr:0018-lot-as-the-preferred-source-extension
- adr:0025-two-targets-python-for-run-llvm-for-build

## Out of scope

- A documentation site, a playground or a second page: the reference stays `reference/lotml.md`.
- A custom domain, analytics or a static-site generator.

## Tasks

- [x] 1.1 (Unit) Write the landing page in `site/` with plain HTML, CSS and a little JS, light and dark themes, readable at phone width
- [x] 1.2 (Unit) Test that every LotML snippet on the page passes `lotml check` and every local link resolves
  _Depends 1.1_
- [x] 1.3 (Unit) Add a Pages workflow that deploys `site/` on a push to `main`, actions pinned by SHA
- [x] 1.4 (Unit) Link the site from the README
- [x] 1.5 (Unit) Present the harness guide: the small model that points an agent at what
      to change, and stays silent when unsure

## Done when

- `uv --directory harness run pytest tests/test_site.py` passes.
- `site/index.html` opened in a browser shows no console errors, in both themes and at 390px wide.
- `.github/workflows/pages.yml` uploads `site/` and deploys it with `actions/deploy-pages`.
