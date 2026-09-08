# zhtw-mcp

[English](README.md) | **繁體中文**

繁體中文（zh-TW）的語言檢查工具，依據教育部標準檢查詞彙、標點與字形。它透過 [Model Context Protocol](https://modelcontextprotocol.io/)（MCP）接上 AI 程式助理，在中國大陸用語（zh-CN）流到使用者眼前之前先攔下來。

工具依循三項台灣官方標準：

- [《重訂標點符號手冊》修訂版](https://language.moe.gov.tw/001/upload/files/site_content/m0001/hau/c2.htm)：標點符號
- [《國字標準字體》](https://language.moe.gov.tw/001/Upload/files/SITE_CONTENT/M0001/STD/F4.HTML)：字形
- 兩岸詞彙正規化，以 [OpenCC](https://github.com/BYVoid/OpenCC) 的 TWPhrases 與 TWVariants 資料集為依據：用字遣詞

超過 1100 條詞彙規則與 15 條大小寫規則編進了執行檔。遇到有歧義的詞，伺服器會問它所在的那個 AI 助理，不需要額外的 API 金鑰。

> **關於這個 fork**：這是 [sysprog21/zhtw-mcp](https://github.com/sysprog21/zhtw-mcp) 的分支，多了一套給 coding agent 用的收尾工作流：`--format agent` 輸出格式與 `zhtw-finalize` skill。詳見〈[給 coding agent 用](#給-coding-agent-用)〉。上游的預先編譯執行檔**不含**這些改動，要用請自行建置。

## 這個專案為什麼存在

### 現代中文的標準化並不完整

清末的知識分子面對一個難題：西方概念要用中文表達，但這套書寫系統裡沒有現成的詞。無論是自己造新詞，還是透過日文轉譯（和製漢語）引進，他們都是在極大的時間壓力下拼湊出一整套文書系統。許多譯詞彼此不一致、語意模糊，甚至互相矛盾。華語世界帶著這些缺陷過了一個多世紀。

### 簡體中文讓情況更糟

中國推動簡化時，削減的不只是筆畫，還有詞彙的精確度。本該依領域區分的詞，全被壓成單一譯法，一個詞打天下。不少譯詞當初訂得倉促：某個詞在一個語境行得通，就未經檢驗地擴散到其他語境。

### AI 模型放大了問題

AI 語言模型學的是網路文字，而其中簡體中文的量遠大於繁體中文（在 [CC-100](https://data.statmt.org/cc-100/) 中約為 2.6 比 1）。[CulturaX](https://huggingface.co/datasets/uonlp/CulturaX) 這類主要資料集甚至沒有把繁體中文單獨計算。一份 [FAccT 2025 的研究](https://arxiv.org/abs/2505.22645)證實，多數模型在被要求寫繁體中文時，仍偏好 zh-CN 的術語。產出看起來像模像樣，卻不是台灣人實際的寫法。

問題不只在字元轉換。同一個詞在海峽兩岸往往指不同的東西：

<!-- zhtw:disable-block -->

| 英文 | zh-CN | zh-TW | 為什麼要緊 |
|------|-------|-------|-----------|
| concurrency | 並發 | 並行 | 在 zh-CN，並行 指的是 parallel，是完全不同的概念 |
| parallel | 並行 | 平行 | zh-CN 的 並行 等於 parallel；在台灣，並行 等於 concurrent |
| process (OS) | 進程 | 行程 | 進程 在台灣是「進度」，不是作業系統的行程 |
| file / document | 文件 / 文檔 | 檔案 / 文件 | 文件 在中國是 file；在台灣是 document |
| render | 渲染 | 算繪 | 渲染 在台灣是「誇大」，源自一種繪畫技法 |
| traverse | 遍歷 | 走訪 | 遍歷 在台灣保留給遍歷理論（Ergodic theory）使用 |

<!-- zhtw:enable -->

### 這個專案做什麼

自動檢查並修正 AI 產出的繁中文字，攔下兩岸術語的滲漏：

<!-- zhtw:disable-block -->

- 該用全形卻用了半形的標點（`,` `.` `:` 應為 `，` `。` `：`）
- 中國式的 `""` 彎引號，改成台灣的 `「」` 直角引號
- CJK 與英數字之間缺少或多餘的空格
- 中國大陸詞彙：軟件→軟體、內存→記憶體、默認→預設等
- 非標準的字形變體：裏→裡、着→著，依教育部標準字體
- 帶政治色彩的詞：祖國、內地
- 大小寫：JavaScript、GitHub、macOS

<!-- zhtw:enable -->

這些標準由嚴格度軸上的兩個 profile 執行，另外搭配彼此正交的能力旗標：

| Profile | 用途 |
|---------|------|
| `base` | 兩岸詞彙、標點、大小寫、文法、政治色彩詞 |
| `strict` | 完整教育部規範：字形變體（裏→裡）、文法（臺／台）、全部標點 |

| 旗標 | 用途 |
|------|------|
| `relaxed` | 為軟體 UI 放寬：關閉冒號與頓號的檢查、關閉文法檢查；範圍改用 en-dash |
| `detect_ai` | AI 寫作審查：填充詞、語意安全詞、繫詞與被動語態、密度型樣式偵測 |

若要處理缺乏來源的權威歸因，在 MCP 選 `document_genre`，在 CLI 用 `--document-genre casual|technical|financial`。這項檢查只在開啟 AI 偵測時執行（`--detect-ai` 或 `detect_ai`），而且在任何文類下都不會建議改寫：刪掉歸因會改變句子主張的內容，所以文類選的是建議，不是改法。一般文章會被建議指名出處或拿掉那句訴諸權威；技術與財經文章則會被告知這項主張需要引用。

Profile 決定繁中規範執行得多嚴，旗標則彼此正交：`detect_ai` 兩個 profile 都能用，`relaxed` 也可以和 `strict` 併用，這樣就能要字形正規化、又對標點寬鬆。

完整規則參考請看 [docs/rules.md](docs/rules.md)。

## 命名慣例：cn 與 tw

本專案遵循 [BCP 47](https://www.rfc-editor.org/info/bcp47)。地區子標籤取自 [ISO 3166-1 alpha-2](https://www.iso.org/iso-3166-country-codes.html)，其中「地區」可以指主權國家、領土或經濟體，不必然是「國家」。

- `zh-CN`：CN 地區書寫的中文（簡體）
- `zh-TW`：TW 地區書寫的中文（繁體）

在整份程式碼中，`cn` 與 `tw` 標示的是地區書寫慣例，不是政治立場。

## 開始使用

### 預先編譯的執行檔

上游每次成功推上 `main` 都會更新滾動式的 [`latest`](https://github.com/sysprog21/zhtw-mcp/releases/tag/latest) release，那也是 GitHub 顯示的最新發行版，不涉及版本標籤。每個壓縮檔內含執行檔、`LICENSE` 與 `README.md`，`SHA256SUMS` 放在旁邊。

| 平台 | 檔案 |
| --- | --- |
| Linux x86_64（glibc 2.39 以上） | `zhtw-mcp-x86_64-unknown-linux-gnu.tar.gz` |
| Linux arm64（glibc 2.39 以上） | `zhtw-mcp-aarch64-unknown-linux-gnu.tar.gz` |
| macOS arm64 | `zhtw-mcp-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `zhtw-mcp-x86_64-pc-windows-msvc.tar.gz` |

其他平台請用下面的 Nix，或從原始程式碼建置。

瀏覽器擴充功能包在同一個 release 裡，檔名 `zhtw-mcp-extension.zip`。解開後開啟開發人員模式，從 `chrome://extensions` 載入。

這些檔案來自上游，**不含本 fork 的 `--format agent` 與 `zhtw-finalize` skill**。需要那些功能請往下看〈從原始程式碼建置〉。

#### macOS / Linux

```bash
base=https://github.com/sysprog21/zhtw-mcp/releases/download/latest
case "$(uname -sm)" in
  "Darwin arm64")  asset=zhtw-mcp-aarch64-apple-darwin.tar.gz ;;
  "Linux x86_64")  asset=zhtw-mcp-x86_64-unknown-linux-gnu.tar.gz ;;
  "Linux aarch64") asset=zhtw-mcp-aarch64-unknown-linux-gnu.tar.gz ;;
  *) asset=""; echo "no pre-built binary for $(uname -sm)" >&2 ;;
esac
[ -n "$asset" ] &&
  curl -fsSLO "$base/$asset" -O "$base/SHA256SUMS" &&
  shasum -a 256 --ignore-missing -c SHA256SUMS &&
  tar -xzf "$asset" zhtw-mcp
```

如果 Linux 上沒有 `shasum`，改用 `sha256sum --ignore-missing -c SHA256SUMS`。

#### Windows（PowerShell）

```powershell
$base = "https://github.com/sysprog21/zhtw-mcp/releases/download/latest"
$asset = "zhtw-mcp-x86_64-pc-windows-msvc.tar.gz"
irm "$base/$asset" -OutFile $asset
irm "$base/SHA256SUMS" -OutFile SHA256SUMS
$want = ((Select-String -Path SHA256SUMS -SimpleMatch $asset).Line -split '\s+')[0]
if ((Get-FileHash -Algorithm SHA256 $asset).Hash -ine $want) { throw "checksum mismatch" }
tar -xzf $asset zhtw-mcp.exe
```

兩段指令都把執行檔留在目前目錄；把它移到 `PATH` 上的某處，就能直接用名字執行。

### Nix

在任何啟用了 Nix 與 flakes 的系統上：

```bash
# 建置後進入一個暫時的 shell，`zhtw-mcp` 已在 `$PATH` 上
nix shell "github:kuo1234/zhtw-mcp"

# 建置並執行 zhtw-mcp（這串指令可以拿去向 MCP 用戶端註冊，見〈安裝〉）
nix run "github:kuo1234/zhtw-mcp"
```

> **注意**：第一次執行會從原始程式碼編譯，可能要幾分鐘。之後會重用 Nix store 的快取，啟動就是瞬間的事。想加快第一次建置，先跑 `nix build --cores 0 "github:kuo1234/zhtw-mcp"`；`--cores 0` 是叫 Nix 用上所有可用的 CPU 核心。

### 從原始程式碼建置

需要 stable Rust 1.91 以上。

```bash
make
```

執行檔在 `target/release/zhtw-mcp`。

Python 3 是建置需求，不只是測試需求：OpenCC 的轉換表由指令碼產生，不在版本庫裡。

### 參與開發

```bash
make check           # CI 跑的那道關卡：測試、clippy、格式、git hook
make indent          # 執行各個格式化工具；關卡檢查的就是它們的結果
make hooks           # 安裝 git hook；uninstall-hooks 移除
make corpus          # 精確率、召回率與偽陽性指標
```

`scripts/indent.sh` 掌管整條格式化鏈：先用 `commentflow` 重排註解，接著 `cargo fmt`、`black`、`shfmt`，最後是 `scripts/check-ruleset.py` 負責的 `assets/ruleset.json` 正規化。`make indent` 以 `--write` 執行它，關卡則以 `--check` 執行，而且是對一份複本執行，所以檢查永遠不會改寫它正在評斷的東西。這條鏈會跑到收斂為止，因為重新縮排一個區塊，可能讓區塊內註解的換行失效。

這裡沒有任何格式化工具吃樣式參數。`shfmt` 從 `.editorconfig` 取設定，`commentflow` 從 `.clang-format` 取欄寬上限，而後者存在的唯一理由就是那個數字：註解在 80 欄換行，`rustfmt` 則允許程式碼到 100。

每一條線在工具缺席時回報略過，而不是失敗，所以本機跑綠燈的證據力比 CI 跑綠燈弱。CI 在 Linux 那一段安裝 `commentflow`、`shfmt` 與 `shellcheck`，並設定 `ZHTW_REQUIRE_TOOLS=1`，把略過變成失敗。

任何一次 `cargo build` 都會透過 `build.rs` 安裝 git hook，`make hooks` 也能單獨安裝。已設定的 `core.hooksPath` 不會被動到，因為它可能由不相干的版本庫共用。pre-commit hook 會對索引的一份檢出執行 `rustfmt`、`black`、`shellcheck`、`shfmt`、`commentflow` 與規則集檢查，所以未暫存的修改既不會讓提交失敗，也不會搭順風車混進去。commit-msg hook 要求標題在 50 欄內、內文在 72 欄內，語氣用祈使句、不含 em dash，並且把一個中日韓字元算成終端機實際佔用的兩欄。pre-push hook 會對 rebase 或 amend 改寫過的提交重跑這些規則，CI 也會對 pull request 自己的提交跑同一支腳本，所以規則一樣約束得到沒裝 hook 的貢獻者。

`scripts/check-comments.sh` 管兩條散文規則，沒有任何格式化工具知道它們：不用 em dash，以及 `///` 或 `//!` 文件註解之外不用反引號，因為在那裡反引號是 rustdoc 標記而不是散文。它在關卡和 pre-commit hook 裡都會跑。

`scripts/test-git-hooks.sh` 用一個臨時版本庫驅動全部四個 hook，並且在關卡裡執行，所以一個不再擋人的 hook 會在那裡被抓到，而不是在別人下一次提交時。

### 安裝

要一次完成建置、安裝到 `$XDG_BIN_HOME`（或 `~/.local/bin`）、停掉舊的伺服器行程，並向偵測到的 MCP 用戶端註冊：

```bash
make install      # 建置 release、安裝執行檔、註冊偵測到的 MCP 用戶端
make uninstall    # 移除執行檔與偵測到的 MCP 註冊
make status       # 檢查執行檔新舊、行程狀態與註冊狀態
```

要手動設定，或用其他 MCP 用戶端：

```bash
# Claude Code
claude mcp add zhtw-mcp -- /path/to/zhtw-mcp

# Codex CLI
codex mcp add zhtw -- /path/to/zhtw-mcp

# OpenCode
opencode mcp add zhtw-mcp /path/to/zhtw-mcp
```

其他 MCP 用戶端可以在專案根目錄放 `.mcp.json`：

```json
{
  "mcpServers": {
    "zhtw-mcp": {
      "command": "/path/to/zhtw-mcp",
      "args": []
    }
  }
}
```

把 `/path/to/zhtw-mcp` 換成實際的執行檔路徑（例如 `target/release/zhtw-mcp`）。

### 給 coding agent 用

走本機 CLI，不要掛 server。Claude Code 和 Codex 有 shell，對它們來說 CLI 是比較便宜的介面：

```bash
zhtw-mcp lint <檔案> --fix --format agent
```

這行會套用確定的修正、重掃它剛寫過的檔案，然後印出 `PASS`，或是每筆殘留一行。`AMBIG` 那幾行是 linter 無法自行判定的，候選全部列出來，交給手上握有整份文件的人決定。

省下來的不是單次回應的大小。MCP server 只要掛著，工具 schema 就會進到每一個請求，整個 session 都是；而一個把「這是中文」當成檢查訊號的 agent，會對自己的 TODO 清單、計畫、分析筆記和交接摘要通通跑一遍，那些東西沒有人會讀。[zhtw-finalize skill](.claude/skills/zhtw-finalize/SKILL.md) 把檢查移到交付的那一刻，並且花了不少篇幅講清楚哪些文件不算交付。`scripts/measure-agent-workflow.py` 會對同一個 session 量測兩條路。

要讓所有專案都吃得到這個 skill：

```bash
cp -r .claude/skills/zhtw-finalize ~/.claude/skills/
```

MCP server 沒有要退場。對於沒有 shell 的 host、結構化與 IDE 整合，以及 Sampling（CLI 沒有對應能力），它仍然是對的介面。

### CLI 快速上手

```bash
zhtw-mcp lint README.md                 # 檢查一個檔案
zhtw-mcp lint file.md --fix             # 就地自動修正
zhtw-mcp lint file.md --fix --dry-run   # 只預覽，不寫檔
zhtw-mcp lint file.md --format agent    # agent 能直接動手的最小報告
zhtw-mcp lint file.md --telemetry       # 在 stderr 印出統計
zhtw-mcp cache clear                    # 清掉持久化的判定快取
```

完整 CLI 參考見 [docs/cli.md](docs/cli.md)，MCP 的工具、資源與提示詞細節見 [docs/mcp.md](docs/mcp.md)。

### 常用提示語

以 MCP server 執行時，你用自然語言互動，助理會把你的意圖轉成 `zhtw` 的工具呼叫：

| 意圖 | 你可以說 | 對應呼叫 | 會發生什麼 |
|------|---------|---------|-----------|
| 檢查文字 | 「幫我看這段有沒有中國用語」 | `zhtw({ "text": "..." })` | 回傳問題的行列位置、建議與規則類型 |
| 自動修正 | 「把這份文件的繁中問題修掉」 | `zhtw({ "text": "...", "fix_mode": "lexical_safe" })` | 套用確定的修正，回傳修好的文字 |
| 品質把關 | 「超過 3 個錯誤就退回」 | `zhtw({ "text": "...", "max_errors": 3 })` | 依錯誤數回傳 `accepted: true/false` |
| 嚴格模式 | 「用教育部嚴格標準檢查」 | `zhtw({ "text": "...", "profile": "strict" })` | 加上字形變體（裏→裡）與完整標點檢查 |
| UI 字串 | 「檢查這句 UI 文字，跳過文法」 | `zhtw({ "text": "...", "relaxed": true })` | 關閉冒號、頓號與文法檢查；範圍用 en-dash |
| AI 寫作審查 | 「看看這段有沒有 AI 味」 | `zhtw({ "text": "...", "detect_ai": true })` | 標出填充詞、語意安全詞、繫詞與被動語態過量 |
| Markdown | 「檢查這份 markdown，跳過程式碼區塊」 | `zhtw({ "text": "...", "content_type": "markdown" })` | 圍籬程式碼、行內程式碼與 HTML 區塊不納入掃描 |
| 成本統計 | 「檢查並附上統計」 | `zhtw({ "text": "...", "include_telemetry": true })` | 回傳這次呼叫的 token 與快取估計值 |

每一次 `zhtw` 呼叫都是無狀態的，`profile` 這類參數是逐次指定，不是 session 狀態。不指定 `profile` 就是 `base`。

伺服器另外開放兩個唯讀資源給助理查閱：`zh-tw://style-guide/moe`（教育部標準）與 `zh-tw://dictionary/ambiguous`（兩岸術語消歧）。完整的提示詞清單見 [docs/mcp.md](docs/mcp.md)。

## 延伸閱讀

- [docs/cli.md](docs/cli.md)：完整 CLI 參考、設定檔、CI/CD 整合、簡繁轉換
- [docs/mcp.md](docs/mcp.md)：MCP 工具參數、資源、提示詞、Sampling、用法範例
- [docs/internals.md](docs/internals.md)：處理流程、字集偵測、設計決策、測試
- [docs/rules.md](docs/rules.md)：規則類型參考、擴充規則集、執行期覆寫

## 授權

`zhtw-mcp` 採用寬鬆的 MIT 式授權。本原始程式碼的使用受 MIT 授權規範，全文見 [LICENSE](LICENSE)。
