use super::*;

fn vim_config() -> Config {
    Config {
        editor: helix_view::editor::Config {
            grammar: helix_view::editor::GrammarMode::Vim,
            lsp: helix_view::editor::LspConfig {
                enable: false,
                ..Default::default()
            },
            ..Default::default()
        },
        keys: helix_term::keymap::default_for_grammar(helix_view::editor::GrammarMode::Vim),
        ..Default::default()
    }
}

fn vim_builder() -> AppBuilder {
    AppBuilder::new().with_config(vim_config())
}

fn vim_test<T: Into<TestCase>>(
    test_case: T,
) -> impl std::future::Future<Output = anyhow::Result<()>> {
    test_with_config(vim_builder(), test_case)
}

// ===================================================================
// Basic motions — cursor moves, no visible selection
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_h_moves_left() -> anyhow::Result<()> {
    vim_test((
        "he#[l|]#lo",
        "h",
        "h#[e|]#llo",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_l_moves_right() -> anyhow::Result<()> {
    vim_test((
        "#[h|]#ello",
        "l",
        "h#[e|]#llo",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_w_moves_to_next_word_start() -> anyhow::Result<()> {
    vim_test((
        "#[h|]#ello world",
        "w",
        "hello #[w|]#orld",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_b_moves_to_prev_word_start() -> anyhow::Result<()> {
    vim_test((
        "hello #[w|]#orld",
        "b",
        "#[h|]#ello world",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_e_moves_to_word_end() -> anyhow::Result<()> {
    vim_test((
        "#[h|]#ello world",
        "e",
        "hell#[o|]# world",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_0_goes_to_line_start() -> anyhow::Result<()> {
    vim_test((
        "  he#[l|]#lo",
        "0",
        "#[ |]# hello",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_caret_goes_to_first_nonwhitespace() -> anyhow::Result<()> {
    vim_test((
        "  he#[l|]#lo",
        "^",
        "  #[h|]#ello",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_dollar_goes_to_line_end() -> anyhow::Result<()> {
    vim_test((
        "#[h|]#ello",
        "$",
        "hell#[o|]#",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_j_moves_down() -> anyhow::Result<()> {
    vim_test((
        indoc! {"\
            #[h|]#ello
            world
        "},
        "j",
        indoc! {"\
            hello
            #[w|]#orld
        "},
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_k_moves_up() -> anyhow::Result<()> {
    vim_test((
        indoc! {"\
            hello
            #[w|]#orld
        "},
        "k",
        indoc! {"\
            #[h|]#ello
            world
        "},
    )).await
}

// ===================================================================
// Operator + motion: dw, de, db, d$, d0
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_dw_deletes_word() -> anyhow::Result<()> {
    vim_test((
        "#[h|]#ello world",
        "dw",
        "#[w|]#orld",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_de_deletes_to_word_end() -> anyhow::Result<()> {
    vim_test((
        "#[h|]#ello world",
        "de",
        "#[ |]#world",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_db_deletes_back_word() -> anyhow::Result<()> {
    vim_test((
        "hello #[w|]#orld",
        "db",
        "#[w|]#orld",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_d_dollar_deletes_to_line_end() -> anyhow::Result<()> {
    vim_test((
        "he#[l|]#lo world",
        "d$",
        "h#[e|]#",
    )).await
}

// ===================================================================
// Doubled operators: dd, yy, cc
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_dd_deletes_line() -> anyhow::Result<()> {
    vim_test((
        indoc! {"\
            #[h|]#ello
            world
        "},
        "dd",
        "#[w|]#orld\n",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_2dd_deletes_two_lines() -> anyhow::Result<()> {
    vim_test((
        indoc! {"\
            #[o|]#ne
            two
            three
        "},
        "2dd",
        "#[t|]#hree\n",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_yy_p_yanks_and_pastes_line() -> anyhow::Result<()> {
    vim_test((
        indoc! {"\
            #[h|]#ello
            world
        "},
        "yyp",
        // TODO: vim puts cursor at first char of pasted line (position 6)
        // helix paste places it at end of pasted content
        indoc! {"\
            hello
            hell#[o|]#
            world
        "},
    )).await
}

// ===================================================================
// Operator + count: d2w
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_d2w_deletes_two_words() -> anyhow::Result<()> {
    vim_test((
        "#[o|]#ne two three",
        "d2w",
        "#[t|]#hree",
    )).await
}

// ===================================================================
// Text objects: ciw, diw, ci", di", ci{, di(
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_ciw_changes_inner_word() -> anyhow::Result<()> {
    vim_test((
        "hello #[w|]#orld end",
        "ciwbye<esc>",
        "hello by#[e|]# end",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_diw_deletes_inner_word() -> anyhow::Result<()> {
    vim_test((
        "hello #[w|]#orld end",
        "diw",
        "hello #[ |]#end",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_ci_quote_changes_inner_quotes() -> anyhow::Result<()> {
    vim_test((
        "say \"h#[e|]#llo\" end",
        "ci\"bye<esc>",
        "say \"by#[e|]#\" end",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_di_quote_deletes_inner_quotes() -> anyhow::Result<()> {
    vim_test((
        "say \"h#[e|]#llo\" end",
        "di\"",
        "say \"#[\"|]# end",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_ci_brace_changes_inner_braces() -> anyhow::Result<()> {
    vim_test((
        "fn() { h#[e|]#llo }",
        "ci{bye<esc>",
        "fn() {by#[e|]#}",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_di_paren_deletes_inner_parens() -> anyhow::Result<()> {
    vim_test((
        "fn(h#[e|]#llo)",
        "di(",
        "fn(#[)|]#",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_caw_changes_around_word() -> anyhow::Result<()> {
    vim_test((
        "hello #[w|]#orld end",
        "cawbye<esc>",
        "hello by#[e|]#end",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_da_quote_deletes_around_quotes() -> anyhow::Result<()> {
    vim_test((
        "say \"h#[e|]#llo\" end",
        "da\"",
        "say #[ |]#end",
    )).await
}

// ===================================================================
// Visual mode
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_v_enters_visual_and_d_deletes() -> anyhow::Result<()> {
    vim_test((
        "#[h|]#ello world",
        "vwd",
        "#[w|]#orld",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_visual_line_and_d_deletes() -> anyhow::Result<()> {
    vim_test((
        indoc! {"\
            #[h|]#ello
            world
            end
        "},
        "Vjd",
        "#[e|]#nd\n",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_viw_selects_inner_word() -> anyhow::Result<()> {
    vim_test((
        "hello #[w|]#orld end",
        "viwd",
        "hello #[ |]#end",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_va_quote_selects_around_quotes() -> anyhow::Result<()> {
    vim_test((
        "say \"h#[e|]#llo\" end",
        "va\"d",
        "say #[ |]#end",
    )).await
}

// ===================================================================
// Linewise motions in operator-pending: dj, dk
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_dj_deletes_two_lines() -> anyhow::Result<()> {
    vim_test((
        indoc! {"\
            #[o|]#ne
            two
            three
        "},
        "dj",
        "#[t|]#hree\n",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_dk_deletes_two_lines_up() -> anyhow::Result<()> {
    vim_test((
        indoc! {"\
            one
            #[t|]#wo
            three
        "},
        "dk",
        "#[t|]#hree\n",
    )).await
}

// ===================================================================
// Special commands: x, X, D, C
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_x_deletes_char() -> anyhow::Result<()> {
    vim_test((
        "#[h|]#ello",
        "x",
        "#[e|]#llo",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_big_x_deletes_char_backward() -> anyhow::Result<()> {
    vim_test((
        "h#[e|]#llo",
        "X",
        "#[e|]#llo",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_big_d_deletes_to_line_end() -> anyhow::Result<()> {
    vim_test((
        "he#[l|]#lo world",
        "D",
        "h#[e|]#",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_big_c_changes_to_line_end() -> anyhow::Result<()> {
    vim_test((
        "he#[l|]#lo world",
        "Cbye<esc>",
        "heby#[e|]#",
    )).await
}

// ===================================================================
// Insert mode entry/exit
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_i_enters_insert() -> anyhow::Result<()> {
    vim_test((
        "h#[e|]#llo",
        "iab<esc>",
        "ha#[b|]#ello",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_a_appends() -> anyhow::Result<()> {
    vim_test((
        "h#[e|]#llo",
        "aab<esc>",
        "hea#[b|]#llo",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_o_opens_below() -> anyhow::Result<()> {
    vim_test((
        indoc! {"\
            #[h|]#ello
            world
        "},
        "onew<esc>",
        indoc! {"\
            hello
            ne#[w|]#
            world
        "},
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_big_o_opens_above() -> anyhow::Result<()> {
    vim_test((
        indoc! {"\
            hello
            #[w|]#orld
        "},
        "Onew<esc>",
        indoc! {"\
            hello
            ne#[w|]#
            world
        "},
    )).await
}

// ===================================================================
// Undo
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_u_undoes_delete_word() -> anyhow::Result<()> {
    // After undo, text is restored; cursor lands at end of restored region
    // TODO: vim puts cursor at start of change (position 0)
    vim_test((
        "#[h|]#ello world",
        "dwu",
        "hello#[ |]#world",
    )).await
}

// ===================================================================
// Dot-repeat (.)
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_dot_repeats_dw() -> anyhow::Result<()> {
    vim_test((
        "#[o|]#ne two three",
        "dw.",
        "#[t|]#hree",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_dot_repeats_dd() -> anyhow::Result<()> {
    vim_test((
        indoc! {"\
            #[o|]#ne
            two
            three
        "},
        "dd.",
        "#[t|]#hree\n",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_dot_repeats_diw() -> anyhow::Result<()> {
    vim_test((
        "#[h|]#ello world",
        "diww.",
        "#[ |]#",
    )).await
}

// ===================================================================
// f/t/F/T motions
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_f_finds_char() -> anyhow::Result<()> {
    vim_test((
        "#[h|]#ello world",
        "fo",
        "hell#[o|]# world",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_t_till_char() -> anyhow::Result<()> {
    vim_test((
        "#[h|]#ello world",
        "to",
        "hel#[l|]#o world",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_big_f_finds_char_backward() -> anyhow::Result<()> {
    vim_test((
        "hello #[w|]#orld",
        "Fl",
        "hel#[l|]#o world",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_df_deletes_to_char() -> anyhow::Result<()> {
    vim_test((
        "#[h|]#ello world",
        "dfo",
        "#[ |]#world",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_dt_deletes_till_char() -> anyhow::Result<()> {
    vim_test((
        "#[a|]#(hello) end",
        "dt)",
        "#[)|]# end",
    )).await
}

// ===================================================================
// gg / G motions
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_gg_goes_to_file_start() -> anyhow::Result<()> {
    vim_test((
        indoc! {"\
            hello
            #[w|]#orld
        "},
        "gg",
        indoc! {"\
            #[h|]#ello
            world
        "},
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_big_g_goes_to_last_line() -> anyhow::Result<()> {
    vim_test((
        indoc! {"\
            #[h|]#ello
            world
        "},
        "G",
        indoc! {"\
            hello
            #[w|]#orld
        "},
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_dgg_deletes_to_file_start() -> anyhow::Result<()> {
    vim_test((
        indoc! {"\
            hello
            #[w|]#orld
            end
        "},
        "dgg",
        "#[e|]#nd\n",
    )).await
}

// ===================================================================
// Paragraph motions: { and }
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_close_brace_moves_to_next_paragraph() -> anyhow::Result<()> {
    vim_test((
        "#[h|]#ello\nworld\n\nend",
        "}",
        "hello\nworld\n#[\n|]#end",
        LineFeedHandling::AsIs,
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_open_brace_moves_to_prev_paragraph() -> anyhow::Result<()> {
    // Helix's paragraph motion goes to the start of the preceding paragraph
    // rather than the blank line itself (unlike vim's { which stops at blank lines).
    vim_test((
        "hello\n\n#[w|]#orld",
        "{",
        "#[h|]#ello\n\nworld",
        LineFeedHandling::AsIs,
    )).await
}

// ===================================================================
// % match bracket
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_percent_matches_bracket() -> anyhow::Result<()> {
    vim_test((
        "#[(|]#hello)",
        "%",
        "(hello#[)|]#",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_d_percent_deletes_to_matching_bracket() -> anyhow::Result<()> {
    vim_test((
        "#[(|]#hello) end",
        "d%",
        "#[ |]#end",
    )).await
}

// ===================================================================
// gu / gU compound operators (case change)
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_gUiw_uppercases_inner_word() -> anyhow::Result<()> {
    vim_test((
        "hello #[w|]#orld end",
        "gUiw",
        "hello #[W|]#ORLD end",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_guiw_lowercases_inner_word() -> anyhow::Result<()> {
    vim_test((
        "hello #[W|]#ORLD end",
        "guiw",
        "hello #[w|]#orld end",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_gUw_uppercases_word() -> anyhow::Result<()> {
    vim_test((
        "#[h|]#ello world",
        "gUw",
        "#[H|]#ELLO world",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_guw_lowercases_word() -> anyhow::Result<()> {
    vim_test((
        "#[H|]#ELLO world",
        "guw",
        "#[h|]#ello world",
    )).await
}

// ===================================================================
// C-v blockwise visual
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_ctrl_v_enters_visual_block() -> anyhow::Result<()> {
    // C-v enters visual mode (block sub-type stored on editor).
    // True block column editing is not yet implemented; for now
    // verify that C-v + esc returns to normal mode cleanly.
    vim_test((
        indoc! {"\
            #[h|]#ello
            world
        "},
        "<C-v><esc>",
        indoc! {"\
            #[h|]#ello
            world
        "},
    )).await
}

// ===================================================================
// Indent / Unindent operators
// ===================================================================

#[tokio::test(flavor = "multi_thread")]
async fn vim_indent_line() -> anyhow::Result<()> {
    vim_test((
        "#[h|]#ello",
        "<gt><gt>",
        "\t#[h|]#ello",
    )).await
}

#[tokio::test(flavor = "multi_thread")]
async fn vim_unindent_line() -> anyhow::Result<()> {
    vim_test((
        "\t#[h|]#ello",
        "<lt><lt>",
        "#[h|]#ello",
    )).await
}
