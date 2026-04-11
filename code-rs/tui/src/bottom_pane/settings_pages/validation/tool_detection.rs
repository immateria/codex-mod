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

pub(crate) fn actionlint_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install actionlint".to_owned();
    }
    if has("brew") {
        return "brew install actionlint".to_owned();
    }
    "See: https://github.com/rhysd/actionlint#installation".to_owned()
}

pub(crate) fn shellcheck_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install shellcheck".to_owned();
    }
    if has("apt-get") {
        return "sudo apt-get update && sudo apt-get install -y shellcheck".to_owned();
    }
    if has("dnf") {
        return "sudo dnf install -y ShellCheck".to_owned();
    }
    if has("yum") {
        return "sudo yum install -y ShellCheck".to_owned();
    }
    if has("brew") {
        return "brew install shellcheck".to_owned();
    }
    "https://www.shellcheck.net/".to_owned()
}

pub(crate) fn markdownlint_hint() -> String {
    if has("npm") {
        return "npm i -g markdownlint-cli2".to_owned();
    }
    if is_macos() && has("brew") {
        return "brew install markdownlint-cli2".to_owned();
    }
    "npm i -g markdownlint-cli2".to_owned()
}

pub(crate) fn hadolint_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install hadolint".to_owned();
    }
    if has("apt-get") {
        return "sudo apt-get update && sudo apt-get install -y hadolint".to_owned();
    }
    if has("dnf") {
        return "sudo dnf install -y hadolint".to_owned();
    }
    if has("yum") {
        return "sudo yum install -y hadolint".to_owned();
    }
    if has("brew") {
        return "brew install hadolint".to_owned();
    }
    "https://github.com/hadolint/hadolint".to_owned()
}

pub(crate) fn yamllint_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install yamllint".to_owned();
    }
    if has("apt-get") {
        return "sudo apt-get update && sudo apt-get install -y yamllint".to_owned();
    }
    if has("dnf") {
        return "sudo dnf install -y yamllint".to_owned();
    }
    if has("yum") {
        return "sudo yum install -y yamllint".to_owned();
    }
    if has("brew") {
        return "brew install yamllint".to_owned();
    }
    "https://yamllint.readthedocs.io/".to_owned()
}

pub(crate) fn cargo_check_hint() -> String {
    if has("cargo") {
        return "cargo check --all-targets".to_owned();
    }
    "Install Rust (https://rustup.rs) to enable cargo check".to_owned()
}

pub(crate) fn shfmt_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install shfmt".to_owned();
    }
    if has("apt-get") {
        return "sudo apt-get update && sudo apt-get install -y shfmt".to_owned();
    }
    if has("dnf") {
        return "sudo dnf install -y shfmt".to_owned();
    }
    if has("yum") {
        return "sudo yum install -y shfmt".to_owned();
    }
    if has("brew") {
        return "brew install shfmt".to_owned();
    }
    "https://github.com/mvdan/sh".to_owned()
}

pub(crate) fn prettier_hint() -> String {
    if has("npm") {
        return "npx --yes prettier --write <path>".to_owned();
    }
    if is_macos() && has("brew") {
        return "brew install prettier".to_owned();
    }
    "npm install --global prettier".to_owned()
}

pub(crate) fn tsc_hint() -> String {
    if has("pnpm") {
        return "pnpm add -D typescript".to_owned();
    }
    if has("yarn") {
        return "yarn add --dev typescript".to_owned();
    }
    "npm install --save-dev typescript".to_owned()
}

pub(crate) fn eslint_hint() -> String {
    if has("pnpm") {
        return "pnpm add -D eslint".to_owned();
    }
    if has("yarn") {
        return "yarn add --dev eslint".to_owned();
    }
    "npm install --save-dev eslint".to_owned()
}

pub(crate) fn phpstan_hint() -> String {
    if has("composer") {
        return "composer require --dev phpstan/phpstan".to_owned();
    }
    "See: https://phpstan.org/user-guide/getting-started".to_owned()
}

pub(crate) fn psalm_hint() -> String {
    if has("composer") {
        return "composer require --dev vimeo/psalm".to_owned();
    }
    "See: https://psalm.dev/docs/install/".to_owned()
}

pub(crate) fn mypy_hint() -> String {
    if has("pipx") {
        return "pipx install mypy".to_owned();
    }
    if has("pip3") {
        return "pip3 install --user mypy".to_owned();
    }
    "pip install --user mypy".to_owned()
}

pub(crate) fn pyright_hint() -> String {
    if has("npm") {
        return "npm install --save-dev pyright".to_owned();
    }
    if has("pipx") {
        return "pipx install pyright".to_owned();
    }
    "See: https://github.com/microsoft/pyright".to_owned()
}

pub(crate) fn golangci_lint_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install golangci-lint".to_owned();
    }
    if has("go") {
        return "go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest".to_owned();
    }
    "https://golangci-lint.run/usage/install/".to_owned()
}

