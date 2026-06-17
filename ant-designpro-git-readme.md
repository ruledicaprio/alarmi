### https://github.com/ant-design/ant-design-pro
ant-design-pro
Public template
ant-design/ant-design-pro
.agents		docs: add CLAUDE.md and AGENTS.md for AI coding agent guidance (#11727)		2 weeks ago
.claude/skills/antd		docs: add CLAUDE.md and AGENTS.md for AI coding agent guidance (#11727)		2 weeks ago
.github		ci: integrate react-doctor for PR health checks (#11777)		5 days ago
.husky		chore: merge v6-full-edition to all-blocks		10 months ago
cloudflare-worker	fix: resolve CF Worker Date.now() returning 0 at module level (#11738)		2 weeks ago
config		feat: offline-friendly error handling with chunk retry and network ba…		last week
docs		docs: add version badge to cheatsheets (#11771)		last week
mock		fix: resolve multiple UI issues (#11678)		last month
public		chore: add biome tooling and format codebase (#11572)		7 months ago
scripts		fix: add getIntl() support to i18n-remove script (#11767)		last week
src		ci: integrate react-doctor for PR health checks (#11777)		5 days ago
tests	Improve mocks for localStorage, Worker, and matchMedia (#11682)		last week
types	fix: resolve multiple UI issues (#11678)	last month
.commitlintrc.js	chore: merge v6-full-edition to all-blocks		10 months ago
.editorconfig		remove unnecessary 'x' permission of some configuration files		8 years ago
.gitignore		ci: integrate react-doctor for PR health checks (#11777)		5 days ago
.lintstagedrc		fix: add getIntl() support to i18n-remove script (#11767)		last week
.npmrc		chore: merge v6-full-edition to all-blocks		10 months ago
AGENTS.md		docs: add CLAUDE.md and AGENTS.md for AI coding agent guidance (#11727)		2 weeks ago
CLAUDE.md		docs: compact CLAUDE.md to reduce context overhead (#11775)		last week
CODE_OF_CONDUCT.md		script: use default prettier		7 years ago
LICENSE		fix: add present to LICENSE (#10092)	4 years ago
README.md		docs: update README and cheatsheet for v6 release	2 weeks ago
README.zh-CN.md		docs: update README and cheatsheet for v6 release 2 weeks ago
biome.json chore: track package-lock.json and update lint config 2 weeks ago
jest.config.ts fix: footer version extra quotes, add Umi/Utoo versions and commit ha… 2 weeks ago
package-lock.json chore(deps): bump mermaid from 11.14.0 to 11.15.0 (#11778) 2 days ago
package.json chore(deps-dev): bump the dependencies group with 3 updates (#11779) 2 days ago
postcss.config.js feat: upgrade to TailwindCSS v4 (#11668) last month
react-doctor.config.json ci: integrate react-doctor for PR health checks (#11777) 5 days ago 
tailwind.config.js refactor: remove duplicate cheatsheet .ts wrappers, import .md direct… 2 weeks ago
tsconfig.json feat: add version and commit hash to footer (#11660) last month
Repository files navigation
	README Code of conduct
	MIT license
Ant Design Pro
	An out-of-box UI solution for enterprise applications as a React boilerplate.
	CI GitHub release Build With Utoo Build With Umi Checked with Biome Ant Design
	Language: 🇺🇸 | 🇨🇳
	light theme preview dark theme preview
	Preview: https://preview.pro.ant.design
	Documentation: docs/cheatsheet.en-US.md
	ChangeLog: https://github.com/ant-design/ant-design-pro/releases
	FAQ: docs/cheatsheet.en-US.md#faq
	v6 Released! — What's new in v6
	Features
💡 TypeScript: A language for application-scale JavaScript
📜 Blocks: Build page with block template
💎 Neat Design: Built on Ant Design 6 specification
📐 Common Templates: Typical templates for enterprise applications
🚀 State of The Art Development: Newest development stack of React 19/Umi Max 4/antd 6/utoopack
📱 Responsive: Designed for variable screen sizes
🎨 Theming: Customizable theme with Tailwind CSS v4 + antd-style
🌐 International: Built-in i18n solution
⚙️ Best Practices: Solid workflow to make your code healthy
🔢 Mock development: Easy to use mock development solution
🤖 AI Assistant: Built-in AI chatbot page powered by Ant Design X
✅ UI Test: Fly safely with unit and e2e tests
Templates
- Welcome
- Dashboard
  - Analysis
  - Monitor
  - Workplace
- Form
  - Basic Form
  - Step Form
  - Advanced Form
- List
  - Search List (Articles/Projects/Applications)
  - Table List
  - Basic List
  - Card List
- Profile
  - Basic Profile
  - Advanced Profile
- Result
  - Success
  - Fail
- Exception
  - 403
  - 404
  - 500
- Account
  - Account Center
  - Account Settings
- AI Assistant
- User
  - Login
  - Register
  - Register Result
Usage
Get Started
Clone or download this repository to your local machine:
git clone --depth=1 https://github.com/ant-design/ant-design-pro.git myapp
cd myapp
Installation
npm install
Development
# Start development server (full version by default)
npm start
Simplify to Simple Version
This project includes all blocks by default. If you need a minimal version, run:
npm run simple
This will:
Remove extra page directories (dashboard, form, list/*, profile, result, exception, account, etc.)
Remove extra mock files
Replace routes with simple version
Remove extra dependencies from package.json
Note: This operation is irreversible and will permanently delete files.
Build
npm run build
Browsers support
Modern browsers.
Edge
Edge	Firefox
Firefox	Chrome
Chrome	Safari
Safari
Edge	last 2 versions	last 2 versions	last 2 versions
