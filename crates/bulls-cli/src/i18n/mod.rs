// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::ffi::OsStr;

use bulls_application::LocalePreference;

mod generated {
    include!(concat!(env!("OUT_DIR"), "/bulls_i18n.rs"));
}

pub(crate) use generated::{Locale, Message};

pub(crate) fn render(locale: Locale, message: Message<'_>) -> String {
    generated::render(locale, message)
}

pub(crate) fn resolve_locale(preference: &LocalePreference) -> Locale {
    let values = [
        env::var_os("LC_ALL"),
        env::var_os("LC_MESSAGES"),
        env::var_os("LANG"),
    ];
    resolve_with_environment(preference, values.iter().map(|value| value.as_deref()))
}

fn resolve_with_environment<'a>(
    preference: &LocalePreference,
    environment: impl IntoIterator<Item = Option<&'a OsStr>>,
) -> Locale {
    if let Some(explicit) = preference.explicit_value() {
        return locale_from_tag(explicit).unwrap_or(generated::DEFAULT_LOCALE);
    }

    environment
        .into_iter()
        .flatten()
        .find(|value| !value.is_empty())
        .and_then(OsStr::to_str)
        .and_then(locale_from_tag)
        .unwrap_or(generated::DEFAULT_LOCALE)
}

fn locale_from_tag(value: &str) -> Option<Locale> {
    let normalized = normalize_locale(value)?;
    if normalized.eq_ignore_ascii_case("C") || normalized.eq_ignore_ascii_case("POSIX") {
        return Some(generated::DEFAULT_LOCALE);
    }

    let selector = normalized.to_ascii_lowercase();
    generated::locale_from_selector(&selector).or_else(|| {
        selector
            .split('-')
            .next()
            .and_then(generated::locale_from_language_fallback)
    })
}

fn normalize_locale(value: &str) -> Option<String> {
    let value = value.split(['.', '@']).next().unwrap_or(value);
    if value.eq_ignore_ascii_case("C") {
        return Some("C".to_owned());
    }
    if value.eq_ignore_ascii_case("POSIX") {
        return Some("POSIX".to_owned());
    }

    let value = value.replace('_', "-");
    let mut normalized = String::with_capacity(value.len());
    for (index, segment) in value.split('-').enumerate() {
        if segment.is_empty()
            || !segment
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
        {
            return None;
        }
        if index != 0 {
            normalized.push('-');
        }

        if index == 0 {
            normalized.extend(
                segment
                    .chars()
                    .map(|character| character.to_ascii_lowercase()),
            );
        } else if segment.len() == 4
            && segment
                .chars()
                .all(|character| character.is_ascii_alphabetic())
        {
            let mut characters = segment.chars();
            let first = characters.next()?;
            normalized.push(first.to_ascii_uppercase());
            normalized.extend(characters.map(|character| character.to_ascii_lowercase()));
        } else if segment.len() == 2
            && segment
                .chars()
                .all(|character| character.is_ascii_alphabetic())
        {
            normalized.extend(
                segment
                    .chars()
                    .map(|character| character.to_ascii_uppercase()),
            );
        } else {
            normalized.extend(
                segment
                    .chars()
                    .map(|character| character.to_ascii_lowercase()),
            );
        }
    }

    Some(normalized)
}

#[cfg(test)]
pub(crate) fn test_locale(tag: &str) -> Locale {
    locale_from_tag(tag).unwrap_or_else(|| panic!("test locale must be supported: {tag}"))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use bulls_application::LocalePreference;

    use super::{Locale, Message, generated, normalize_locale, render, resolve_with_environment};

    #[test]
    fn generated_catalog_api_is_typed_and_manifest_driven() {
        assert_eq!(generated::I18N_SCHEMA_VERSION, 1);
        assert_eq!(
            generated::SUPPORTED_LOCALES,
            &[
                Locale::EnUs,
                Locale::PtBr,
                Locale::Es,
                Locale::De,
                Locale::Ja,
                Locale::ZhHans,
            ]
        );
        assert_eq!(generated::DEFAULT_LOCALE, Locale::EnUs);
        assert_eq!(Locale::EnUs.tag(), "en-US");
        assert_eq!(Locale::Es.tag(), "es");
        assert_eq!(Locale::De.tag(), "de");
        assert_eq!(Locale::Ja.tag(), "ja");
        assert_eq!(Locale::ZhHans.tag(), "zh-Hans");
        assert_eq!(generated::locale_from_selector("pt-br"), Some(Locale::PtBr));
        assert_eq!(
            generated::locale_from_selector("zh-cn"),
            Some(Locale::ZhHans)
        );
        assert_eq!(
            generated::locale_from_language_fallback("en"),
            Some(Locale::EnUs)
        );
        assert_eq!(
            generated::locale_from_language_fallback("es"),
            Some(Locale::Es)
        );
        assert_eq!(
            generated::locale_from_language_fallback("de"),
            Some(Locale::De)
        );
        assert_eq!(
            generated::locale_from_language_fallback("ja"),
            Some(Locale::Ja)
        );
        assert_eq!(generated::locale_from_language_fallback("zh"), None);
        assert_eq!(render(Locale::PtBr, Message::Error), "Erro");
        assert_eq!(render(Locale::EnUs, Message::Discovery), "Discovery");
        assert_eq!(
            render(
                Locale::PtBr,
                Message::ErrorLimitOutOfRange { maximum: "500" }
            ),
            "O valor de --limit deve estar entre 1 e 500."
        );
        assert_eq!(
            render(Locale::Es, Message::ErrorUnknownOption { option: "--wat" }),
            "Opción desconocida: --wat"
        );
        assert_eq!(
            render(Locale::De, Message::DiscoveryRootsRequested { count: "12" }),
            "Angeforderte Wurzeln: 12"
        );
        assert_eq!(render(Locale::Ja, Message::HelpSectionUsage), "使用方法");
        assert_eq!(
            render(
                Locale::ZhHans,
                Message::ConfigurationGitProcessTimeout {
                    milliseconds: "8500",
                }
            ),
            "Git 进程超时: 8500 ms"
        );
        assert_eq!(
            render(
                Locale::EnUs,
                Message::ErrorUnknownOption { option: "--wat" }
            ),
            "Unknown option: --wat"
        );
        assert_eq!(
            render(
                Locale::PtBr,
                Message::ErrorRuntime {
                    error_code: "operation_failed",
                }
            ),
            "A operação do BullSaddle falhou: operation_failed."
        );
        assert_eq!(
            render(Locale::PtBr, Message::FailureClassOperational),
            "operacional"
        );
        assert_eq!(
            render(Locale::PtBr, Message::FailureCauseTimedOut),
            "tempo limite excedido"
        );
        assert_eq!(
            render(Locale::PtBr, Message::ObservationFailureTimedOut),
            "tempo esgotado"
        );
        assert_eq!(
            render(Locale::PtBr, Message::OperationStatusPartial),
            "Status: parcial"
        );
        assert_eq!(
            render(
                Locale::EnUs,
                Message::DiscoveryRootsRequested { count: "12" }
            ),
            "Roots requested: 12"
        );
        assert_eq!(
            render(
                Locale::PtBr,
                Message::ConfigurationGitProcessTimeout {
                    milliseconds: "8500",
                }
            ),
            "Timeout de processo Git: 8500 ms"
        );
        assert_eq!(
            render(
                Locale::PtBr,
                Message::RepositoriesPagination {
                    shown: "10",
                    offset: "20",
                    total: "42",
                }
            ),
            "Exibindo 10 repositórios a partir do offset 20 de 42 repositórios correspondentes"
        );
        assert_eq!(
            render(
                Locale::EnUs,
                Message::ObservationStatusFailedLastSuccessComplete {
                    failure: "timed out",
                    attempted_age: "1m ago",
                    success_age: "2m ago",
                }
            ),
            "Observation: failed: timed out — 1m ago; last successful observation 2m ago (complete)"
        );
        assert_eq!(
            render(
                Locale::PtBr,
                Message::AdvisoryWorktreeKnownUpstreamDivergence {
                    worktree_id: "worktree-1",
                    ahead: "2",
                    behind: "1",
                }
            ),
            "worktree worktree-1: 2 commits locais estão à frente do upstream conhecido localmente (atrás 1)"
        );
        assert_eq!(render(Locale::PtBr, Message::HelpSectionUsage), "Uso");
        assert_eq!(
            render(
                Locale::EnUs,
                Message::HelpRepositoriesOptionLimit {
                    default: "50",
                    maximum: "500",
                }
            ),
            "Return at most n repositories. Default: 50. Maximum: 500."
        );
    }

    #[test]
    fn locale_normalization_handles_platform_forms_without_selecting_a_locale() {
        let cases = [
            ("pt_BR.UTF-8", "pt-BR"),
            ("ja_JP.UTF-8", "ja-JP"),
            ("de_DE", "de-DE"),
            ("zh_CN.UTF-8", "zh-CN"),
            ("zh_hans_CN@modifier", "zh-Hans-CN"),
            ("C.UTF-8", "C"),
            ("posix", "POSIX"),
        ];

        for (raw, expected) in cases {
            assert_eq!(normalize_locale(raw).as_deref(), Some(expected));
        }
        assert_eq!(normalize_locale("pt--BR"), None);
        assert_eq!(normalize_locale(""), None);
    }

    #[test]
    fn explicit_locale_has_precedence_over_environment() {
        let preference = LocalePreference::explicit("pt-BR").expect("locale must be valid");

        assert_eq!(
            resolve_with_environment(
                &preference,
                [
                    Some(OsStr::new("en_US.UTF-8")),
                    Some(OsStr::new("en_GB.UTF-8")),
                    Some(OsStr::new("en")),
                ],
            ),
            Locale::PtBr
        );
    }

    #[test]
    fn environment_precedence_is_lc_all_then_lc_messages_then_lang() {
        let preference = LocalePreference::auto();

        assert_eq!(
            resolve_with_environment(
                &preference,
                [
                    Some(OsStr::new("pt_BR.UTF-8")),
                    Some(OsStr::new("en_US.UTF-8")),
                    Some(OsStr::new("en")),
                ],
            ),
            Locale::PtBr
        );
        assert_eq!(
            resolve_with_environment(
                &preference,
                [
                    Some(OsStr::new("")),
                    Some(OsStr::new("pt_BR.UTF-8")),
                    Some(OsStr::new("en_US.UTF-8")),
                ],
            ),
            Locale::PtBr
        );
        assert_eq!(
            resolve_with_environment(&preference, [None, None, Some(OsStr::new("en_GB.UTF-8"))],),
            Locale::EnUs
        );
    }

    #[test]
    fn matching_uses_manifest_aliases_and_explicit_language_fallbacks() {
        let preference = LocalePreference::auto();

        for value in ["pt_BR.UTF-8", "pt-BR", "pt"] {
            assert_eq!(
                resolve_with_environment(&preference, [Some(OsStr::new(value))]),
                Locale::PtBr
            );
        }
        for value in ["es_ES.UTF-8", "es_MX.UTF-8", "es"] {
            assert_eq!(
                resolve_with_environment(&preference, [Some(OsStr::new(value))]),
                Locale::Es
            );
        }
        assert_eq!(
            resolve_with_environment(&preference, [Some(OsStr::new("de_DE.UTF-8"))]),
            Locale::De
        );
        assert_eq!(
            resolve_with_environment(&preference, [Some(OsStr::new("ja_JP.UTF-8"))]),
            Locale::Ja
        );
        for value in ["zh_CN.UTF-8", "zh_SG.UTF-8", "zh-Hans"] {
            assert_eq!(
                resolve_with_environment(&preference, [Some(OsStr::new(value))]),
                Locale::ZhHans
            );
        }
        assert_eq!(
            resolve_with_environment(&preference, [Some(OsStr::new("en_GB.UTF-8"))]),
            Locale::EnUs
        );
        assert_eq!(
            resolve_with_environment(&preference, [Some(OsStr::new("pt_PT.UTF-8"))]),
            Locale::EnUs
        );
    }

    #[test]
    fn unsupported_and_posix_locales_fall_back_to_manifest_default() {
        let automatic = LocalePreference::auto();
        let explicit = LocalePreference::explicit("fr-FR").expect("locale must be valid");

        for value in [
            "C",
            "C.UTF-8",
            "POSIX",
            "fr_FR.UTF-8",
            "zh_TW.UTF-8",
            "zh_HK.UTF-8",
            "zh_Hant.UTF-8",
        ] {
            assert_eq!(
                resolve_with_environment(&automatic, [Some(OsStr::new(value))]),
                Locale::EnUs
            );
        }
        assert_eq!(
            resolve_with_environment(&explicit, [Some(OsStr::new("pt_BR.UTF-8"))]),
            Locale::EnUs
        );
    }

    #[test]
    fn higher_precedence_unsupported_locale_does_not_fall_through_to_lower_environment() {
        let preference = LocalePreference::auto();

        assert_eq!(
            resolve_with_environment(
                &preference,
                [
                    Some(OsStr::new("fr_FR.UTF-8")),
                    Some(OsStr::new("pt_BR.UTF-8")),
                    Some(OsStr::new("pt_BR.UTF-8")),
                ],
            ),
            Locale::EnUs
        );
    }
}
