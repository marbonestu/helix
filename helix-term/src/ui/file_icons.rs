use std::path::Path;

/// Returns `(icon, theme_scope)` for a given filename/extension.
pub fn icon_for_file(name: &str) -> (&'static str, &'static str) {
    // Check special filenames first
    let lower = name.to_lowercase();
    if let Some(icon) = match_special_filename(&lower) {
        return icon;
    }

    // Check by extension
    if let Some(ext) = Path::new(name).extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_lowercase();
        if let Some(icon) = match_extension(&ext_lower) {
            return icon;
        }
    }

    // Default file icon
    ("\u{f15b}", "ui.sidebar.icon") //
}

/// Returns `(icon, theme_scope)` for a directory.
pub fn icon_for_directory(expanded: bool) -> (&'static str, &'static str) {
    if expanded {
        ("\u{f0770}", "ui.sidebar.icon.directory") // 󰝰
    } else {
        ("\u{f024b}", "ui.sidebar.icon.directory") // 󰉋
    }
}

fn match_special_filename(name: &str) -> Option<(&'static str, &'static str)> {
    Some(match name {
        "makefile" | "gnumakefile" => ("\u{e673}", "ui.sidebar.icon.makefile"),
        "dockerfile" | "containerfile" => ("\u{f308}", "ui.sidebar.icon.docker"),
        "docker-compose.yml" | "docker-compose.yaml" | "compose.yml" | "compose.yaml" => {
            ("\u{f308}", "ui.sidebar.icon.docker")
        }
        "cargo.toml" | "cargo.lock" => ("\u{e7a8}", "ui.sidebar.icon.rust"),
        "package.json" | "package-lock.json" => ("\u{e718}", "ui.sidebar.icon.javascript"),
        "tsconfig.json" => ("\u{e628}", "ui.sidebar.icon.typescript"),
        "go.mod" | "go.sum" => ("\u{e626}", "ui.sidebar.icon.go"),
        "gemfile" | "rakefile" => ("\u{e791}", "ui.sidebar.icon.ruby"),
        "license" | "licence" => ("\u{f0219}", "ui.sidebar.icon"),
        ".gitignore" | ".gitmodules" | ".gitattributes" => ("\u{e702}", "ui.sidebar.icon.git"),
        ".env" | ".env.local" | ".env.example" => ("\u{f462}", "ui.sidebar.icon"),
        "flake.nix" | "flake.lock" => ("\u{f313}", "ui.sidebar.icon.nix"),
        "readme.md" | "readme.txt" | "readme" => ("\u{f48a}", "ui.sidebar.icon.markdown"),
        "justfile" => ("\u{e673}", "ui.sidebar.icon.makefile"),
        _ => return None,
    })
}

fn match_extension(ext: &str) -> Option<(&'static str, &'static str)> {
    Some(match ext {
        // Rust
        "rs" => ("\u{e7a8}", "ui.sidebar.icon.rust"),
        // Python
        "py" | "pyi" | "pyw" => ("\u{e606}", "ui.sidebar.icon.python"),
        // JavaScript
        "js" | "mjs" | "cjs" => ("\u{e74e}", "ui.sidebar.icon.javascript"),
        "jsx" => ("\u{e7ba}", "ui.sidebar.icon.javascript"),
        // TypeScript
        "ts" | "mts" | "cts" => ("\u{e628}", "ui.sidebar.icon.typescript"),
        "tsx" => ("\u{e7ba}", "ui.sidebar.icon.typescript"),
        // Go
        "go" => ("\u{e626}", "ui.sidebar.icon.go"),
        // Java / JVM
        "java" => ("\u{e738}", "ui.sidebar.icon.java"),
        "kt" | "kts" => ("\u{e634}", "ui.sidebar.icon.kotlin"),
        "scala" | "sc" => ("\u{e737}", "ui.sidebar.icon.scala"),
        "clj" | "cljs" | "cljc" => ("\u{e768}", "ui.sidebar.icon.clojure"),
        // C / C++
        "c" => ("\u{e61e}", "ui.sidebar.icon.c"),
        "h" => ("\u{e61e}", "ui.sidebar.icon.c"),
        "cpp" | "cc" | "cxx" => ("\u{e61d}", "ui.sidebar.icon.cpp"),
        "hpp" | "hh" | "hxx" => ("\u{e61d}", "ui.sidebar.icon.cpp"),
        // C#
        "cs" => ("\u{f031b}", "ui.sidebar.icon.csharp"),
        // Ruby
        "rb" => ("\u{e791}", "ui.sidebar.icon.ruby"),
        // PHP
        "php" => ("\u{e608}", "ui.sidebar.icon.php"),
        // Swift
        "swift" => ("\u{e755}", "ui.sidebar.icon.swift"),
        // Zig
        "zig" => ("\u{e6a9}", "ui.sidebar.icon.zig"),
        // Elixir / Erlang
        "ex" | "exs" => ("\u{e62d}", "ui.sidebar.icon.elixir"),
        "erl" | "hrl" => ("\u{e7b1}", "ui.sidebar.icon.erlang"),
        // Haskell
        "hs" | "lhs" => ("\u{e777}", "ui.sidebar.icon.haskell"),
        // OCaml
        "ml" | "mli" => ("\u{e67a}", "ui.sidebar.icon.ocaml"),
        // Lua
        "lua" => ("\u{e620}", "ui.sidebar.icon.lua"),
        // Shell
        "sh" | "bash" | "zsh" | "fish" => ("\u{e795}", "ui.sidebar.icon.shell"),
        // Nix
        "nix" => ("\u{f313}", "ui.sidebar.icon.nix"),
        // Web
        "html" | "htm" => ("\u{e736}", "ui.sidebar.icon.html"),
        "css" => ("\u{e749}", "ui.sidebar.icon.css"),
        "scss" | "sass" => ("\u{e603}", "ui.sidebar.icon.css"),
        "vue" => ("\u{e6a0}", "ui.sidebar.icon.vue"),
        "svelte" => ("\u{e697}", "ui.sidebar.icon.svelte"),
        // Data / Config
        "json" | "jsonc" => ("\u{e60b}", "ui.sidebar.icon.json"),
        "toml" => ("\u{e6b2}", "ui.sidebar.icon.toml"),
        "yaml" | "yml" => ("\u{e6a8}", "ui.sidebar.icon.yaml"),
        "xml" | "svg" => ("\u{f05c0}", "ui.sidebar.icon.xml"),
        "csv" | "tsv" => ("\u{f0219}", "ui.sidebar.icon"),
        "sql" => ("\u{e706}", "ui.sidebar.icon.database"),
        "graphql" | "gql" => ("\u{e662}", "ui.sidebar.icon.graphql"),
        "proto" => ("\u{e6b2}", "ui.sidebar.icon"),
        // Markdown / Docs
        "md" | "mdx" => ("\u{f48a}", "ui.sidebar.icon.markdown"),
        "txt" => ("\u{f15b}", "ui.sidebar.icon"),
        "rst" => ("\u{f15b}", "ui.sidebar.icon"),
        "tex" | "latex" => ("\u{e69b}", "ui.sidebar.icon.tex"),
        // Docker
        "dockerfile" => ("\u{f308}", "ui.sidebar.icon.docker"),
        // Git
        "diff" | "patch" => ("\u{e702}", "ui.sidebar.icon.git"),
        // Images
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico" | "webp" => {
            ("\u{f1c5}", "ui.sidebar.icon.image")
        }
        // Lock files
        "lock" => ("\u{f023}", "ui.sidebar.icon"),
        // Terraform
        "tf" | "tfvars" => ("\u{e69a}", "ui.sidebar.icon.terraform"),
        // Misc
        "vim" => ("\u{e62b}", "ui.sidebar.icon.vim"),
        "el" | "elc" => ("\u{e779}", "ui.sidebar.icon.elisp"),
        "r" | "rmd" => ("\u{e68a}", "ui.sidebar.icon.r"),
        "dart" => ("\u{e798}", "ui.sidebar.icon.dart"),
        "wasm" => ("\u{e6a1}", "ui.sidebar.icon.wasm"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_file_icon() {
        let (icon, scope) = icon_for_file("main.rs");
        assert_eq!(scope, "ui.sidebar.icon.rust");
        assert!(!icon.is_empty());
    }

    #[test]
    fn test_special_filename() {
        let (_, scope) = icon_for_file("Cargo.toml");
        assert_eq!(scope, "ui.sidebar.icon.rust");
    }

    #[test]
    fn test_unknown_extension_returns_default() {
        let (icon, scope) = icon_for_file("something.xyz123");
        assert_eq!(scope, "ui.sidebar.icon");
        assert_eq!(icon, "\u{f15b}");
    }

    #[test]
    fn test_directory_icons() {
        let (_, scope) = icon_for_directory(false);
        assert_eq!(scope, "ui.sidebar.icon.directory");
        let (_, scope) = icon_for_directory(true);
        assert_eq!(scope, "ui.sidebar.icon.directory");
    }

    #[test]
    fn test_case_insensitive_special_filename() {
        let (_, scope) = icon_for_file("DOCKERFILE");
        assert_eq!(scope, "ui.sidebar.icon.docker");
    }

    #[test]
    fn test_case_insensitive_extension() {
        let (_, scope) = icon_for_file("App.JS");
        assert_eq!(scope, "ui.sidebar.icon.javascript");
    }
}
