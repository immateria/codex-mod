use std::ffi::OsStr;
use std::path::PathBuf;

use code_core::config_types::validation_tool_category;

use super::ToolStatus;

pub(crate) fn detect_tools() -> Vec<ToolStatus> {
    vec![
        ToolStatus {
            name: "actionlint",
            description: "Lint GitHub workflows for syntax and logic issues.",
            installed: has("actionlint"),
            install_hint: actionlint_hint(),
            category: validation_tool_category("actionlint"),
        },
        ToolStatus {
            name: "shellcheck",
            description: "Analyze shell scripts for bugs and common pitfalls.",
            installed: has("shellcheck"),
            install_hint: shellcheck_hint(),
            category: validation_tool_category("shellcheck"),
        },
        ToolStatus {
            name: "markdownlint",
            description: "Lint Markdown content for style and formatting problems.",
            installed: has("markdownlint"),
            install_hint: markdownlint_hint(),
            category: validation_tool_category("markdownlint"),
        },
        ToolStatus {
            name: "hadolint",
            description: "Lint Dockerfiles for best practices and mistakes.",
            installed: has("hadolint"),
            install_hint: hadolint_hint(),
            category: validation_tool_category("hadolint"),
        },
        ToolStatus {
            name: "yamllint",
            description: "Validate YAML files for syntax issues.",
            installed: has("yamllint"),
            install_hint: yamllint_hint(),
            category: validation_tool_category("yamllint"),
        },
        ToolStatus {
            name: "cargo-check",
            description: "Run `cargo check` to catch Rust compilation errors quickly.",
            installed: has("cargo"),
            install_hint: cargo_check_hint(),
            category: validation_tool_category("cargo-check"),
        },
        ToolStatus {
            name: "tsc",
            description: "Type-check TypeScript projects with `tsc --noEmit`.",
            installed: has("tsc"),
            install_hint: tsc_hint(),
            category: validation_tool_category("tsc"),
        },
        ToolStatus {
            name: "eslint",
            description: "Lint JavaScript/TypeScript with ESLint (no warnings allowed).",
            installed: has("eslint"),
            install_hint: eslint_hint(),
            category: validation_tool_category("eslint"),
        },
        ToolStatus {
            name: "mypy",
            description: "Static type-check Python files using mypy.",
            installed: has("mypy"),
            install_hint: mypy_hint(),
            category: validation_tool_category("mypy"),
        },
        ToolStatus {
            name: "pyright",
            description: "Run Pyright for fast Python type analysis.",
            installed: has("pyright"),
            install_hint: pyright_hint(),
            category: validation_tool_category("pyright"),
        },
        ToolStatus {
            name: "phpstan",
            description: "Analyze PHP code with phpstan using project rules.",
            installed: has("phpstan"),
            install_hint: phpstan_hint(),
            category: validation_tool_category("phpstan"),
        },
        ToolStatus {
            name: "psalm",
            description: "Run Psalm to detect PHP runtime issues.",
            installed: has("psalm"),
            install_hint: psalm_hint(),
            category: validation_tool_category("psalm"),
        },
        ToolStatus {
            name: "golangci-lint",
            description: "Lint Go modules with golangci-lint.",
            installed: has("golangci-lint"),
            install_hint: golangci_lint_hint(),
            category: validation_tool_category("golangci-lint"),
        },
        ToolStatus {
            name: "shfmt",
            description: "Format shell scripts consistently with shfmt.",
            installed: has("shfmt"),
            install_hint: shfmt_hint(),
            category: validation_tool_category("shfmt"),
        },
        ToolStatus {
            name: "prettier",
            description: "Format web assets (JS/TS/JSON/MD) with Prettier.",
            installed: has("prettier"),
            install_hint: prettier_hint(),
            category: validation_tool_category("prettier"),
        },
    ]
}

fn which(exe: &str) -> Option<PathBuf> {
    let name = OsStr::new(exe);
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn has(cmd: &str) -> bool {
    which(cmd).is_some()
}

fn is_macos() -> bool {
    cfg!(target_os = "macos")
}

pub fn actionlint_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install actionlint".to_string();
    }
    if has("brew") {
        return "brew install actionlint".to_string();
    }
    "See: https://github.com/rhysd/actionlint#installation".to_string()
}

pub fn shellcheck_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install shellcheck".to_string();
    }
    if has("apt-get") {
        return "sudo apt-get update && sudo apt-get install -y shellcheck".to_string();
    }
    if has("dnf") {
        return "sudo dnf install -y ShellCheck".to_string();
    }
    if has("yum") {
        return "sudo yum install -y ShellCheck".to_string();
    }
    if has("brew") {
        return "brew install shellcheck".to_string();
    }
    "https://www.shellcheck.net/".to_string()
}

pub fn markdownlint_hint() -> String {
    if has("npm") {
        return "npm i -g markdownlint-cli2".to_string();
    }
    if is_macos() && has("brew") {
        return "brew install markdownlint-cli2".to_string();
    }
    "npm i -g markdownlint-cli2".to_string()
}

pub fn hadolint_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install hadolint".to_string();
    }
    if has("apt-get") {
        return "sudo apt-get update && sudo apt-get install -y hadolint".to_string();
    }
    if has("dnf") {
        return "sudo dnf install -y hadolint".to_string();
    }
    if has("yum") {
        return "sudo yum install -y hadolint".to_string();
    }
    if has("brew") {
        return "brew install hadolint".to_string();
    }
    "https://github.com/hadolint/hadolint".to_string()
}

pub fn yamllint_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install yamllint".to_string();
    }
    if has("apt-get") {
        return "sudo apt-get update && sudo apt-get install -y yamllint".to_string();
    }
    if has("dnf") {
        return "sudo dnf install -y yamllint".to_string();
    }
    if has("yum") {
        return "sudo yum install -y yamllint".to_string();
    }
    if has("brew") {
        return "brew install yamllint".to_string();
    }
    "https://yamllint.readthedocs.io/".to_string()
}

pub fn cargo_check_hint() -> String {
    if has("cargo") {
        return "cargo check --all-targets".to_string();
    }
    "Install Rust (https://rustup.rs) to enable cargo check".to_string()
}

pub fn shfmt_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install shfmt".to_string();
    }
    if has("apt-get") {
        return "sudo apt-get update && sudo apt-get install -y shfmt".to_string();
    }
    if has("dnf") {
        return "sudo dnf install -y shfmt".to_string();
    }
    if has("yum") {
        return "sudo yum install -y shfmt".to_string();
    }
    if has("brew") {
        return "brew install shfmt".to_string();
    }
    "https://github.com/mvdan/sh".to_string()
}

pub fn prettier_hint() -> String {
    if has("npm") {
        return "npx --yes prettier --write <path>".to_string();
    }
    if is_macos() && has("brew") {
        return "brew install prettier".to_string();
    }
    "npm install --global prettier".to_string()
}

pub fn tsc_hint() -> String {
    if has("pnpm") {
        return "pnpm add -D typescript".to_string();
    }
    if has("yarn") {
        return "yarn add --dev typescript".to_string();
    }
    "npm install --save-dev typescript".to_string()
}

pub fn eslint_hint() -> String {
    if has("pnpm") {
        return "pnpm add -D eslint".to_string();
    }
    if has("yarn") {
        return "yarn add --dev eslint".to_string();
    }
    "npm install --save-dev eslint".to_string()
}

pub fn phpstan_hint() -> String {
    if has("composer") {
        return "composer require --dev phpstan/phpstan".to_string();
    }
    "See: https://phpstan.org/user-guide/getting-started".to_string()
}

pub fn psalm_hint() -> String {
    if has("composer") {
        return "composer require --dev vimeo/psalm".to_string();
    }
    "See: https://psalm.dev/docs/install/".to_string()
}

pub fn mypy_hint() -> String {
    if has("pipx") {
        return "pipx install mypy".to_string();
    }
    if has("pip3") {
        return "pip3 install --user mypy".to_string();
    }
    "pip install --user mypy".to_string()
}

pub fn pyright_hint() -> String {
    if has("npm") {
        return "npm install --save-dev pyright".to_string();
    }
    if has("pipx") {
        return "pipx install pyright".to_string();
    }
    "See: https://github.com/microsoft/pyright".to_string()
}

pub fn golangci_lint_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install golangci-lint".to_string();
    }
    if has("go") {
        return "go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest".to_string();
    }
    "https://golangci-lint.run/usage/install/".to_string()
}

