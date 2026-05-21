# 本地端口管理器 · Local Port Manager

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Platform](https://img.shields.io/badge/platform-Windows-0078d4.svg)
![Built with](https://img.shields.io/badge/built%20with-Tauri%202-ffc131.svg)
![Rust](https://img.shields.io/badge/rust-stable-orange.svg)
![Frontend](https://img.shields.io/badge/frontend-vanilla%20JS-yellow.svg)

> 一个把"我机器上的端口到底都谁占着"这事一次性看清楚的小工具。
> A small Windows app that finally tells you who's holding which port on your machine.

---

## 中文版

### 你是不是也头疼过

新建一个本地服务，挑端口靠运气——5000 被占了试 5001，5001 也被占试 5002，查端口又记不住 `netstat -ano` 后面接什么。前阵子装的那个 Redis 是不是把 6379 占了？我之前那个 Flask 项目是 5000 还是 5001 来着？

各种开发工具像野草一样在机器上越长越多，每个都默默挑了几个端口塞自己用，自己根本理不清。这玩意儿就是给这种局面准备的：**把你机器上跟端口相关的一切扔到一个界面里。**

### 它都能做什么

- 🔍 **实时监听端口**：TCP / UDP 全收，带 PID、进程名、可执行路径，关键字筛选
- 🎯 **空闲段推荐**：严格判定——5 类来源都不占的端口段；上一段／下一段／随机一段三个按钮翻着选
- 🔎 **单端口查询**：输入端口号一查到底，告诉你它在哪些来源中被占、被谁占
- 📁 **配置文件扫描**：你指定路径，它走遍里头的 ini / yaml / json / conf / toml / properties，把声明用到的端口提取出来
- 🧠 **本地 LLM 精炼** *(可选)*：接 LM Studio 的本地小模型，扫出来的候选让它帮你判别真假端口、自动打软件标签，省得手动取舍几千条噪音
- 💾 **声明库**：一处管理「我装的 X 用了 Y 端口」，可编辑标签、删除、一键清空
- 🚫 **Windows 系统排除段**：netsh 拿到的 Hyper-V / WSL / WinNAT 保留段一并展示，避免你"为啥这端口绑不上"的疑惑

### 五个"端口被占"的来源

推荐空闲段的功能背后是一套**严格**的"什么算占用"——一个端口必须同时满足这五个来源都没占，才算"完全空闲"：

1. **IANA 公认端口表** — 内置 100+ 条，从 ssh / http 这种到 redis / postgres 这种
2. **Windows 系统排除段** — 实时跑 `netsh int ipv4 show excludedportrange`
3. **当前监听端口** — Windows API 拿到的所有 TCP / UDP listening sockets
4. **内置工具默认表** — Vite / Flask / Django / Jenkins / Spring Boot 等常见开发工具的默认端口
5. **你声明的端口** — 扫描配置文件入库的 + 你手动编辑过的

### 关于 LLM 精炼

正则匹配端口这事天然两难——卡得严了真端口漏抓，卡得松了 `127.0.0.1` 里的 `1` 都当端口报上来。于是加了 LLM 复核：扫描完，把每条候选连上下文交给本地大模型，让它说"这是不是真端口、对应哪个软件"。LM Studio 没开就退化到纯正则，不影响正常使用。

强烈建议在 LM Studio 里加载**非推理模型**（Qwen2.5-7B-Instruct / Llama-3-8B-Instruct / Mistral-7B-Instruct 这种），不要用 Qwen3 / DeepSeek R1 / QwQ 这些会自己思考几千字的——会把 token 全烧在 thinking 上、最后什么答案都吐不出。

### 怎么跑起来

需要 [Rust 工具链](https://rustup.rs/) + [Node.js](https://nodejs.org/) + Windows 10/11 上的 MSVC Build Tools。

```bash
git clone https://github.com/qwnzkkldsfai/port-manager
cd port-manager
npm install
npm run tauri build -- --no-bundle
```

产出在 `src-tauri/target/release/port-manager-app.exe`，约 9.4 MB，双击启动。

> 部分进程的可执行路径（尤其是系统服务）需要管理员权限才能看到名字。app 右上角有「重启为管理员」按钮，或右键 exe → 以管理员身份运行。

### 技术栈

- **后端**：Rust + Tauri 2（WebView 渲染前端，不起 HTTP 端口）
- **前端**：原生 HTML / CSS / JS，不用框架
- **端口枚举**：`netstat2` + `sysinfo`
- **配置扫描**：`walkdir` + `regex`
- **LLM 集成**：`reqwest` 调 OpenAI 兼容接口（`json_schema` 优先、`text` 兜底）

### 协议

[MIT License](LICENSE) — 随意用、改、卖、塞进闭源产品，留着署名就行。

---

## English

### Sound familiar?

You're spinning up a new local service. What port? "Try 5000... taken. 5001... also taken. 5002..." Half an hour later you give up and Google `netstat` flags again. Did that Redis you installed last month grab 6379? Was your old Flask project on 5000 or 5001?

Dev tools spread across your machine like weeds, each silently claiming a few ports. This tool is for that mess: **every port-related fact on your machine, in one screen.**

### What it does

- 🔍 **Live listening list** — TCP / UDP, with PID, process name, executable path, keyword filter
- 🎯 **Free-segment recommender** — strict: only ports nobody claims across all 5 sources. Prev / Next / Random buttons to cycle
- 🔎 **Single-port lookup** — type a number, see exactly which sources claim it and who owns it
- 📁 **Config file scanning** — walks ini / yaml / json / conf / toml / properties files at a path you specify and extracts declared ports
- 🧠 **Local-LLM refinement** *(optional)* — wire up LM Studio so a local model judges real ports vs noise and auto-labels which software owns each, sparing you from manually unchecking thousands of false positives
- 💾 **Declaration library** — manage "tool X uses port Y" entries, editable, deletable, one-click clear
- 🚫 **Windows excluded ranges** — surfaces Hyper-V / WSL / WinNAT reservations from netsh so you stop wondering why a bind fails

### The 5 sources of "occupied"

The recommendation feature rests on a strict definition: a port counts as **completely free** only if all five sources are clear.

1. **IANA well-known list** — 100+ bundled, from ssh / http to redis / postgres
2. **Windows excluded ranges** — live from `netsh int ipv4 show excludedportrange`
3. **Currently listening sockets** — all TCP / UDP listeners via Windows API
4. **Built-in dev tool defaults** — Vite / Flask / Django / Jenkins / Spring Boot, etc.
5. **Your declarations** — what config-file scanning committed, plus your manual edits

### About the LLM step

Regex port-matching has a built-in dilemma: too strict and you miss real ports written in unusual ways; too loose and `127.0.0.1`'s `1` becomes a "port candidate". So an LLM review step is layered on: after scanning, each candidate (plus context) goes to a local model that decides "is this a real port, and which software owns it?" If LM Studio isn't running, the tool falls back to pure regex — no smart filter, but otherwise normal.

Strongly recommended: use a **non-reasoning** model in LM Studio (Qwen2.5-7B-Instruct, Llama-3-8B-Instruct, Mistral-7B-Instruct, etc.). Avoid Qwen3 / DeepSeek R1 / QwQ — they burn the token budget on internal reasoning and never produce a final answer.

### Build from source

Requires [Rust toolchain](https://rustup.rs/) + [Node.js](https://nodejs.org/) + MSVC Build Tools on Windows 10/11.

```bash
git clone https://github.com/qwnzkkldsfai/port-manager
cd port-manager
npm install
npm run tauri build -- --no-bundle
```

Output is at `src-tauri/target/release/port-manager-app.exe`, around 9.4 MB. Double-click to launch.

> Some process info (executable paths of system services) requires admin rights. The top bar has a "Restart as Administrator" button, or right-click the exe → Run as administrator.

### Tech stack

- **Backend**: Rust + Tauri 2 (WebView frontend, no HTTP port bound)
- **Frontend**: Vanilla HTML / CSS / JS, no framework
- **Port enumeration**: `netstat2` + `sysinfo`
- **Config scanning**: `walkdir` + `regex`
- **LLM integration**: `reqwest` hitting an OpenAI-compatible endpoint (`json_schema` first, `text` fallback)

### License

[MIT](LICENSE) — use, modify, sell, embed in proprietary work. Just keep the attribution.
