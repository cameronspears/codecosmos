# *c o s m o s*

An AI-powered assistant that reviews your code and suggests improvements — right in your terminal.

Cosmos reads your project, finds things that could be better, and helps you fix them. No complex setup. No IDE required. Just run it and go.

## What Cosmos Does

- **Scans your code** and finds areas that could be improved
- **Explains issues in plain English** — no jargon
- **Suggests fixes** and lets you preview changes before applying
- **Creates pull requests** so changes go through your normal review process

**Supported languages:** JavaScript, TypeScript, Python, Rust, Go

---

## Installation

### Mac

1. Go to the [**Releases page**](https://github.com/cameronspears/cosmos/releases/latest)
2. Download **cosmos-macos-installer.pkg**
3. Double-click the downloaded file to install
4. When prompted, enter your password to complete installation

That's it — Cosmos is now installed.

### Windows

1. Go to the [**Releases page**](https://github.com/cameronspears/cosmos/releases/latest)
2. Download **cosmos-windows-installer.exe**
3. Run the installer
4. If Windows shows a security prompt, click "More info" then "Run anyway"
5. Follow the installation wizard

That's it — Cosmos is now installed.

### Linux

**Ubuntu/Debian:**

Open Terminal and run:
```bash
curl -fsSL https://raw.githubusercontent.com/cameronspears/cosmos/main/install.sh | bash
```

**Other distributions:**

Same command works on most Linux systems.

---

## Getting Started

### Step 1: Open your project in Terminal

Cosmos needs to run from inside your project folder. Here's how:

**Mac:**
1. Open the **Terminal** app (search for "Terminal" in Spotlight)
2. Type `cd ` (with a space after it)
3. Drag your project folder from Finder into the Terminal window
4. Press Enter

**Windows:**
1. Open **PowerShell** (search for it in Start menu)
2. Type `cd ` (with a space after it)
3. Type the path to your project, like `C:\Users\YourName\Projects\my-app`
4. Press Enter

**Example:**
```bash
cd /Users/yourname/Projects/my-website
```

### Step 2: Run Cosmos

Once you're in your project folder, just type:

```bash
cosmos
```

### Step 3: Set up AI features (first time only)

The first time you run Cosmos, it will ask you to set up an API key. This is what powers the AI suggestions.

1. Cosmos will show you a link to get a free API key
2. Follow the link, create an account, and copy your key
3. Paste the key when Cosmos asks for it

Your key is saved securely on your computer. You won't need to enter it again.

---

## Using Cosmos

When Cosmos starts, you'll see a list of suggestions for your project.

### Navigation

| Key | What it does |
|-----|--------------|
| `↑` `↓` | Move up and down the list |
| `Enter` | View details or apply a suggestion |
| `Tab` | Switch between panels |
| `?` | Show help |
| `q` | Quit Cosmos |

### Working with suggestions

1. **Browse suggestions** — Use arrow keys to look through the list
2. **View details** — Press `Enter` on any suggestion to see more
3. **Apply a fix** — When viewing a suggestion, press `Enter` to preview and apply the fix
4. **Undo** — Press `u` to undo the last change you applied

### Other features

| Key | What it does |
|-----|--------------|
| `/` | Search through suggestions |
| `i` | Ask Cosmos a question about your code |
| `g` | Toggle between grouped and flat view |
| `Esc` | Go back or cancel |

---

## How Fixes Work

When you apply a suggestion:

1. **Preview** — Cosmos shows you exactly what will change
2. **Apply** — Creates a new branch with the fix
3. **Review** — Cosmos checks the fix for any issues
4. **Ship** — Commit, push, and create a pull request

This keeps your main code safe. All changes go through your normal review process.

---

## Suggestion Priority

Cosmos marks suggestions by importance:

| Icon | Meaning |
|------|---------|
| `!!` | High priority — significant improvement |
| `!` | Medium priority — worth considering |
| (blank) | Low priority — minor enhancement |

---

## Troubleshooting

### "Command not found" when running cosmos

Make sure you're running the command in Terminal (Mac/Linux) or PowerShell (Windows). If you just installed Cosmos, try closing and reopening your terminal.

### Cosmos shows no suggestions

Make sure you're running Cosmos from inside a project folder that contains code files. Cosmos works with JavaScript, TypeScript, Python, Rust, and Go files.

### API key issues

If Cosmos can't find your API key, you can set it up again:

```bash
cosmos --setup
```

---

## Quick Reference

```bash
# Run Cosmos in current folder
cosmos

# Run Cosmos on a specific project
cosmos /path/to/your/project

# Set up or change your API key
cosmos --setup

# Show project statistics (no interactive mode)
cosmos --stats
```

---

## Privacy

- Your code is sent to the AI service only when generating suggestions
- Your API key is stored securely in your system's keychain
- Cosmos caches results locally to minimize API usage and costs

---

## License

MIT

---

*"A contemplative companion for your codebase"*
