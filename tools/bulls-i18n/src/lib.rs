// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::{self, Write as _};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const I18N_SCHEMA_VERSION: u16 = 1;

#[derive(Debug)]
pub enum I18nError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    InvalidArgument(String),
    Validation(ValidationReport),
}

impl fmt::Display for I18nError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(formatter, "failed to access {}: {source}", path.display())
            }
            Self::Json { path, source } => {
                write!(formatter, "invalid JSON in {}: {source}", path.display())
            }
            Self::InvalidArgument(message) => formatter.write_str(message),
            Self::Validation(report) => {
                writeln!(formatter, "i18n validation failed:")?;
                for issue in report.issues() {
                    writeln!(formatter, "  {}: {}", issue.code(), issue.message())?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for I18nError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationIssue {
    code: &'static str,
    message: String,
}

impl ValidationIssue {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ValidationReport {
    issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    fn push(&mut self, code: &'static str, message: impl Into<String>) {
        self.issues.push(ValidationIssue::new(code, message));
    }

    pub fn is_valid(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn issues(&self) -> &[ValidationIssue] {
        &self.issues
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoverageRow {
    message: String,
    locale_status: Vec<bool>,
}

impl CoverageRow {
    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn locale_status(&self) -> &[bool] {
        &self.locale_status
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoverageReport {
    locales: Vec<String>,
    rows: Vec<CoverageRow>,
}

impl CoverageReport {
    pub fn locales(&self) -> &[String] {
        &self.locales
    }

    pub fn rows(&self) -> &[CoverageRow] {
        &self.rows
    }

    pub fn locale_totals(&self) -> Vec<usize> {
        (0..self.locales.len())
            .map(|index| {
                self.rows
                    .iter()
                    .filter(|row| row.locale_status[index])
                    .count()
            })
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MissingMessage {
    message: String,
    reason: &'static str,
}

impl MissingMessage {
    pub fn message(&self) -> &str {
        &self.message
    }

    pub const fn reason(&self) -> &'static str {
        self.reason
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Manifest {
    schema_version: u16,
    default_locale: String,
    locales: Vec<LocaleEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LocaleEntry {
    tag: String,
    name: String,
    aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    language_fallback: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct MessageContract {
    schema_version: u16,
    messages: UniqueMap<MessageSpec>,
}

#[derive(Clone, Debug, Deserialize)]
struct MessageSpec {
    parameters: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Catalog {
    locale: String,
    messages: UniqueMap<String>,
}

#[derive(Clone, Debug)]
struct UniqueMap<T>(BTreeMap<String, T>);

impl<T> UniqueMap<T> {
    fn get(&self, key: &str) -> Option<&T> {
        self.0.get(key)
    }

    fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.keys()
    }

    fn iter(&self) -> impl Iterator<Item = (&String, &T)> {
        self.0.iter()
    }
}

impl<T> Serialize for UniqueMap<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

impl<'de, T> Deserialize<'de> for UniqueMap<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UniqueMapVisitor<T>(std::marker::PhantomData<T>);

        impl<'de, T> Visitor<'de> for UniqueMapVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = UniqueMap<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object with unique keys")
            }

            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = BTreeMap::new();
                while let Some((key, value)) = access.next_entry::<String, T>()? {
                    if values.insert(key.clone(), value).is_some() {
                        return Err(serde::de::Error::custom(format!("duplicate key `{key}`")));
                    }
                }
                Ok(UniqueMap(values))
            }
        }

        deserializer.deserialize_map(UniqueMapVisitor(std::marker::PhantomData))
    }
}

struct CatalogWorkspace {
    root: PathBuf,
    manifest: Manifest,
    contract: MessageContract,
    catalogs: BTreeMap<String, Catalog>,
    catalog_files: BTreeMap<String, PathBuf>,
}

pub fn validate(root: &Path) -> Result<ValidationReport, I18nError> {
    let workspace = CatalogWorkspace::load(root)?;
    Ok(workspace.validation_report())
}

pub fn compile(root: &Path, output: &Path) -> Result<(), I18nError> {
    let workspace = CatalogWorkspace::load(root)?;
    let report = workspace.validation_report();
    if !report.is_valid() {
        return Err(I18nError::Validation(report));
    }

    let generated = workspace.generate_rust()?;
    fs::write(output, generated).map_err(|source| I18nError::Io {
        path: output.to_path_buf(),
        source,
    })
}

pub fn coverage(root: &Path) -> Result<CoverageReport, I18nError> {
    let workspace = CatalogWorkspace::load(root)?;
    let locales = workspace
        .manifest
        .locales
        .iter()
        .map(|locale| locale.tag.clone())
        .collect::<Vec<_>>();
    let rows = workspace
        .contract
        .messages
        .iter()
        .map(|(message, spec)| CoverageRow {
            message: message.clone(),
            locale_status: locales
                .iter()
                .map(|locale| workspace.translation_is_valid(locale, message, spec))
                .collect(),
        })
        .collect();

    Ok(CoverageReport { locales, rows })
}

pub fn missing(root: &Path, locale: &str) -> Result<Vec<MissingMessage>, I18nError> {
    let workspace = CatalogWorkspace::load(root)?;
    if !workspace
        .manifest
        .locales
        .iter()
        .any(|entry| entry.tag == locale)
    {
        return Err(I18nError::InvalidArgument(format!(
            "locale `{locale}` is not registered"
        )));
    }

    let catalog = workspace.catalogs.get(locale);
    let mut missing = Vec::new();
    for (message, spec) in workspace.contract.messages.iter() {
        let reason = match catalog.and_then(|catalog| catalog.messages.get(message)) {
            None => Some("missing"),
            Some(text) if text.is_empty() => Some("empty"),
            Some(text) if placeholders(text).is_err() => Some("invalid placeholders"),
            Some(text) if !placeholder_set_matches(text, &spec.parameters) => {
                Some("placeholder mismatch")
            }
            Some(_) => None,
        };
        if let Some(reason) = reason {
            missing.push(MissingMessage {
                message: message.clone(),
                reason,
            });
        }
    }
    Ok(missing)
}

pub fn add_locale(root: &Path, locale: &str) -> Result<PathBuf, I18nError> {
    if !valid_locale_tag(locale) {
        return Err(I18nError::InvalidArgument(format!(
            "invalid locale tag `{locale}`"
        )));
    }

    let manifest_path = root.join("manifest.json");
    let messages_path = root.join("messages.json");
    let mut manifest: Manifest = read_json(&manifest_path)?;
    let contract: MessageContract = read_json(&messages_path)?;

    if manifest.locales.iter().any(|entry| {
        entry.tag.eq_ignore_ascii_case(locale)
            || entry
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(locale))
    }) {
        return Err(I18nError::InvalidArgument(format!(
            "locale or alias `{locale}` is already registered"
        )));
    }

    let catalog_path = root.join("locales").join(format!("{locale}.json"));
    if catalog_path.exists() {
        return Err(I18nError::InvalidArgument(format!(
            "catalog already exists: {}",
            catalog_path.display()
        )));
    }

    manifest.locales.push(LocaleEntry {
        tag: locale.to_owned(),
        name: locale.to_owned(),
        aliases: vec![locale.to_owned()],
        language_fallback: None,
    });

    let catalog = Catalog {
        locale: locale.to_owned(),
        messages: UniqueMap(
            contract
                .messages
                .keys()
                .cloned()
                .map(|message| (message, String::new()))
                .collect(),
        ),
    };

    write_json(&manifest_path, &manifest)?;
    write_json(&catalog_path, &catalog)?;
    Ok(catalog_path)
}

impl CatalogWorkspace {
    fn load(root: &Path) -> Result<Self, I18nError> {
        let manifest_path = root.join("manifest.json");
        let messages_path = root.join("messages.json");
        let locales_path = root.join("locales");
        let manifest = read_json(&manifest_path)?;
        let contract = read_json(&messages_path)?;
        let mut catalogs = BTreeMap::new();
        let mut catalog_files = BTreeMap::new();

        let entries = fs::read_dir(&locales_path).map_err(|source| I18nError::Io {
            path: locales_path.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| I18nError::Io {
                path: locales_path.clone(),
                source,
            })?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let file_name = path
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    I18nError::InvalidArgument(format!(
                        "catalog filename is not valid UTF-8: {}",
                        path.display()
                    ))
                })?
                .to_owned();
            let catalog = read_json(&path)?;
            catalogs.insert(file_name.clone(), catalog);
            catalog_files.insert(file_name, path);
        }

        Ok(Self {
            root: root.to_path_buf(),
            manifest,
            contract,
            catalogs,
            catalog_files,
        })
    }

    fn validation_report(&self) -> ValidationReport {
        let mut report = ValidationReport::default();
        self.validate_schema_versions(&mut report);
        self.validate_manifest(&mut report);
        self.validate_contract(&mut report);
        self.validate_catalogs(&mut report);
        report
    }

    fn validate_schema_versions(&self, report: &mut ValidationReport) {
        if self.manifest.schema_version != I18N_SCHEMA_VERSION {
            report.push(
                "manifest_schema_version",
                format!(
                    "unsupported manifest schema version {}; expected {}",
                    self.manifest.schema_version, I18N_SCHEMA_VERSION
                ),
            );
        }
        if self.contract.schema_version != I18N_SCHEMA_VERSION {
            report.push(
                "message_schema_version",
                format!(
                    "unsupported message schema version {}; expected {}",
                    self.contract.schema_version, I18N_SCHEMA_VERSION
                ),
            );
        }
    }

    fn validate_manifest(&self, report: &mut ValidationReport) {
        let mut canonical = HashMap::<String, String>::new();
        let mut selectors = HashMap::<String, String>::new();
        let mut language_fallbacks = Vec::<(String, String)>::new();
        let mut variants = HashMap::<String, String>::new();

        for locale in &self.manifest.locales {
            if !valid_locale_tag(&locale.tag) {
                report.push(
                    "invalid_locale_tag",
                    format!("invalid canonical locale tag `{}`", locale.tag),
                );
            }
            if locale.name.trim().is_empty() {
                report.push(
                    "empty_locale_name",
                    format!("locale `{}` has an empty human name", locale.tag),
                );
            }

            let normalized_tag = locale.tag.to_ascii_lowercase();
            if let Some(previous) = canonical.insert(normalized_tag.clone(), locale.tag.clone()) {
                report.push(
                    "duplicate_locale",
                    format!("locale `{}` duplicates `{previous}`", locale.tag),
                );
            }
            register_selector(
                report,
                &mut selectors,
                &normalized_tag,
                &locale.tag,
                "canonical locale tag",
            );

            let variant = rust_type_identifier(&locale.tag);
            match variant {
                Some(variant) => {
                    if let Some(previous) = variants.insert(variant.clone(), locale.tag.clone()) {
                        report.push(
                            "locale_identifier_collision",
                            format!(
                                "locales `{previous}` and `{}` both generate Rust identifier `{variant}`",
                                locale.tag
                            ),
                        );
                    }
                }
                None => report.push(
                    "invalid_locale_identifier",
                    format!(
                        "locale `{}` cannot generate a stable Rust identifier",
                        locale.tag
                    ),
                ),
            }

            let mut local_aliases = HashSet::new();
            for alias in &locale.aliases {
                if !valid_locale_tag(alias) {
                    report.push(
                        "invalid_locale_alias",
                        format!("locale `{}` has invalid alias `{alias}`", locale.tag),
                    );
                    continue;
                }
                let normalized = alias.to_ascii_lowercase();
                if !local_aliases.insert(normalized.clone()) {
                    report.push(
                        "duplicate_locale_alias",
                        format!("locale `{}` repeats alias `{alias}`", locale.tag),
                    );
                    continue;
                }
                register_selector(
                    report,
                    &mut selectors,
                    &normalized,
                    &locale.tag,
                    "locale alias",
                );
            }

            if let Some(fallback) = &locale.language_fallback {
                if valid_language_fallback(fallback) {
                    language_fallbacks.push((fallback.to_ascii_lowercase(), locale.tag.clone()));
                } else {
                    report.push(
                        "invalid_language_fallback",
                        format!(
                            "locale `{}` has invalid language fallback `{fallback}`",
                            locale.tag
                        ),
                    );
                }
            }
        }

        let mut fallback_owners = HashMap::<String, String>::new();
        for (fallback, locale) in language_fallbacks {
            if let Some(previous) = fallback_owners.insert(fallback.clone(), locale.clone())
                && previous != locale
            {
                report.push(
                    "ambiguous_language_fallback",
                    format!(
                        "language fallback `{fallback}` for locale `{locale}` conflicts with locale `{previous}`"
                    ),
                );
            }
            if let Some(selector_locale) = selectors.get(&fallback)
                && selector_locale != &locale
            {
                report.push(
                    "language_fallback_selector_conflict",
                    format!(
                        "language fallback `{fallback}` for locale `{locale}` conflicts with selector owned by locale `{selector_locale}`"
                    ),
                );
            }
        }

        if !canonical.contains_key(&self.manifest.default_locale.to_ascii_lowercase()) {
            report.push(
                "missing_default_locale",
                format!(
                    "default locale `{}` is not registered",
                    self.manifest.default_locale
                ),
            );
        }
    }

    fn validate_contract(&self, report: &mut ValidationReport) {
        let mut variants = HashMap::<String, String>::new();
        for (message, spec) in self.contract.messages.iter() {
            let Some(variant) = rust_message_identifier(message) else {
                report.push(
                    "invalid_message_identifier",
                    format!("message `{message}` cannot generate a stable Rust identifier"),
                );
                continue;
            };
            if let Some(previous) = variants.insert(variant.clone(), message.clone()) {
                report.push(
                    "message_identifier_collision",
                    format!(
                        "messages `{previous}` and `{message}` both generate Rust identifier `{variant}`"
                    ),
                );
            }

            let mut parameters = HashSet::new();
            for parameter in &spec.parameters {
                if !valid_parameter_name(parameter) || rust_keyword(parameter) {
                    report.push(
                        "invalid_parameter",
                        format!("message `{message}` has invalid parameter `{parameter}`"),
                    );
                } else if !parameters.insert(parameter) {
                    report.push(
                        "duplicate_parameter",
                        format!("message `{message}` repeats parameter `{parameter}`"),
                    );
                }
            }
        }
    }

    fn validate_catalogs(&self, report: &mut ValidationReport) {
        let expected = self
            .manifest
            .locales
            .iter()
            .map(|locale| locale.tag.as_str())
            .collect::<HashSet<_>>();

        for locale in &self.manifest.locales {
            let Some(catalog) = self.catalogs.get(&locale.tag) else {
                report.push(
                    "missing_catalog",
                    format!("locale `{}` has no catalog file", locale.tag),
                );
                continue;
            };
            if catalog.locale != locale.tag {
                report.push(
                    "catalog_locale_mismatch",
                    format!(
                        "catalog `{}` declares locale `{}`",
                        locale.tag, catalog.locale
                    ),
                );
            }
            self.validate_catalog_messages(&locale.tag, catalog, report);
        }

        for (file_name, path) in &self.catalog_files {
            if !expected.contains(file_name.as_str()) {
                report.push(
                    "unexpected_catalog",
                    format!(
                        "catalog {} is not registered in the manifest",
                        path.strip_prefix(&self.root).unwrap_or(path).display()
                    ),
                );
            }
        }
    }

    fn validate_catalog_messages(
        &self,
        locale: &str,
        catalog: &Catalog,
        report: &mut ValidationReport,
    ) {
        for (message, spec) in self.contract.messages.iter() {
            let Some(text) = catalog.messages.get(message) else {
                report.push(
                    "missing_message",
                    format!("locale `{locale}` is missing message `{message}`"),
                );
                continue;
            };
            if text.is_empty() {
                report.push(
                    "empty_translation",
                    format!("locale `{locale}` has empty message `{message}`"),
                );
                continue;
            }
            match placeholders(text) {
                Ok(found) => {
                    let expected = spec.parameters.iter().cloned().collect::<BTreeSet<_>>();
                    let found = found.into_iter().collect::<BTreeSet<_>>();
                    for missing in expected.difference(&found) {
                        report.push(
                            "missing_placeholder",
                            format!(
                                "locale `{locale}` message `{message}` is missing placeholder `{{{missing}}}`"
                            ),
                        );
                    }
                    for additional in found.difference(&expected) {
                        report.push(
                            "additional_placeholder",
                            format!(
                                "locale `{locale}` message `{message}` has unexpected placeholder `{{{additional}}}`"
                            ),
                        );
                    }
                }
                Err(reason) => report.push(
                    "malformed_placeholder",
                    format!("locale `{locale}` message `{message}`: {reason}"),
                ),
            }
        }

        for message in catalog.messages.keys() {
            if self.contract.messages.get(message).is_none() {
                report.push(
                    "unknown_message",
                    format!("locale `{locale}` defines unknown message `{message}`"),
                );
            }
        }
    }

    fn translation_is_valid(&self, locale: &str, message: &str, spec: &MessageSpec) -> bool {
        self.catalogs
            .get(locale)
            .and_then(|catalog| catalog.messages.get(message))
            .is_some_and(|text| !text.is_empty() && placeholder_set_matches(text, &spec.parameters))
    }

    fn generate_rust(&self) -> Result<String, I18nError> {
        let mut output = String::new();
        writeln!(
            output,
            "// BullSaddle — Developed by Orion Impact (https://orion-impact.com)"
        )
        .expect("writing to String cannot fail");
        writeln!(output, "// Licensed under the Apache License, Version 2.0.")
            .expect("writing to String cannot fail");
        writeln!(output, "// SPDX-License-Identifier: Apache-2.0\n")
            .expect("writing to String cannot fail");
        writeln!(output, "#[cfg(test)]").expect("writing to String cannot fail");
        writeln!(
            output,
            "pub const I18N_SCHEMA_VERSION: u16 = {};",
            self.manifest.schema_version
        )
        .expect("writing to String cannot fail");
        writeln!(output, "\n#[derive(Clone, Copy, Debug, Eq, PartialEq)]")
            .expect("writing to String cannot fail");
        writeln!(output, "pub enum Locale {{").expect("writing to String cannot fail");
        for locale in &self.manifest.locales {
            writeln!(
                output,
                "    {},",
                rust_type_identifier(&locale.tag).expect("validated locale identifier")
            )
            .expect("writing to String cannot fail");
        }
        writeln!(output, "}}\n").expect("writing to String cannot fail");

        writeln!(output, "#[cfg(test)]").expect("writing to String cannot fail");
        writeln!(output, "impl Locale {{").expect("writing to String cannot fail");
        writeln!(output, "    pub const fn tag(self) -> &'static str {{")
            .expect("writing to String cannot fail");
        writeln!(output, "        match self {{").expect("writing to String cannot fail");
        for locale in &self.manifest.locales {
            writeln!(
                output,
                "            Self::{} => {:?},",
                rust_type_identifier(&locale.tag).expect("validated locale identifier"),
                locale.tag
            )
            .expect("writing to String cannot fail");
        }
        writeln!(output, "        }}").expect("writing to String cannot fail");
        writeln!(output, "    }}").expect("writing to String cannot fail");
        writeln!(output, "}}\n").expect("writing to String cannot fail");

        writeln!(output, "#[cfg(test)]").expect("writing to String cannot fail");
        writeln!(output, "pub const SUPPORTED_LOCALES: &[Locale] = &[")
            .expect("writing to String cannot fail");
        for locale in &self.manifest.locales {
            writeln!(
                output,
                "    Locale::{},",
                rust_type_identifier(&locale.tag).expect("validated locale identifier")
            )
            .expect("writing to String cannot fail");
        }
        writeln!(output, "];\n").expect("writing to String cannot fail");

        let default_locale = self
            .manifest
            .locales
            .iter()
            .find(|locale| {
                locale
                    .tag
                    .eq_ignore_ascii_case(&self.manifest.default_locale)
            })
            .expect("validated default locale must exist");
        let default_variant =
            rust_type_identifier(&default_locale.tag).expect("validated locale identifier");
        writeln!(
            output,
            "pub const DEFAULT_LOCALE: Locale = Locale::{default_variant};\n"
        )
        .expect("writing to String cannot fail");

        let mut selectors = BTreeMap::<String, String>::new();
        let mut language_fallbacks = BTreeMap::<String, String>::new();
        for locale in &self.manifest.locales {
            let variant = rust_type_identifier(&locale.tag).expect("validated locale identifier");
            selectors.insert(locale.tag.to_ascii_lowercase(), variant.clone());
            for alias in &locale.aliases {
                selectors.insert(alias.to_ascii_lowercase(), variant.clone());
            }
            if let Some(fallback) = &locale.language_fallback {
                language_fallbacks.insert(fallback.to_ascii_lowercase(), variant);
            }
        }

        writeln!(
            output,
            "pub fn locale_from_selector(selector: &str) -> Option<Locale> {{"
        )
        .expect("writing to String cannot fail");
        writeln!(output, "    match selector {{").expect("writing to String cannot fail");
        for (selector, variant) in selectors {
            writeln!(output, "        {selector:?} => Some(Locale::{variant}),")
                .expect("writing to String cannot fail");
        }
        writeln!(output, "        _ => None,").expect("writing to String cannot fail");
        writeln!(output, "    }}").expect("writing to String cannot fail");
        writeln!(output, "}}\n").expect("writing to String cannot fail");

        writeln!(
            output,
            "pub fn locale_from_language_fallback(language: &str) -> Option<Locale> {{"
        )
        .expect("writing to String cannot fail");
        writeln!(output, "    match language {{").expect("writing to String cannot fail");
        for (language, variant) in language_fallbacks {
            writeln!(output, "        {language:?} => Some(Locale::{variant}),")
                .expect("writing to String cannot fail");
        }
        writeln!(output, "        _ => None,").expect("writing to String cannot fail");
        writeln!(output, "    }}").expect("writing to String cannot fail");
        writeln!(output, "}}\n").expect("writing to String cannot fail");

        writeln!(output, "#[derive(Clone, Copy, Debug, Eq, PartialEq)]")
            .expect("writing to String cannot fail");
        writeln!(output, "pub enum Message<'a> {{").expect("writing to String cannot fail");
        for (message, spec) in self.contract.messages.iter() {
            let variant = rust_message_identifier(message).expect("validated message identifier");
            if spec.parameters.is_empty() {
                writeln!(output, "    {variant},").expect("writing to String cannot fail");
            } else {
                writeln!(output, "    {variant} {{").expect("writing to String cannot fail");
                for parameter in &spec.parameters {
                    writeln!(output, "        {parameter}: &'a str,")
                        .expect("writing to String cannot fail");
                }
                writeln!(output, "    }},").expect("writing to String cannot fail");
            }
        }
        writeln!(output, "}}\n").expect("writing to String cannot fail");

        writeln!(
            output,
            "pub fn render(locale: Locale, message: Message<'_>) -> String {{"
        )
        .expect("writing to String cannot fail");
        writeln!(output, "    match (locale, message) {{").expect("writing to String cannot fail");
        for locale in &self.manifest.locales {
            let locale_variant =
                rust_type_identifier(&locale.tag).expect("validated locale identifier");
            let catalog = self
                .catalogs
                .get(&locale.tag)
                .expect("validated catalog must exist");
            for (message, spec) in self.contract.messages.iter() {
                let message_variant =
                    rust_message_identifier(message).expect("validated message identifier");
                let pattern = if spec.parameters.is_empty() {
                    format!("Message::{message_variant}")
                } else {
                    format!(
                        "Message::{message_variant} {{ {} }}",
                        spec.parameters.join(", ")
                    )
                };
                writeln!(
                    output,
                    "        (Locale::{locale_variant}, {pattern}) => {{"
                )
                .expect("writing to String cannot fail");
                let text = catalog
                    .messages
                    .get(message)
                    .expect("validated translation must exist");
                emit_render_body(&mut output, text).map_err(I18nError::InvalidArgument)?;
                writeln!(output, "        }}").expect("writing to String cannot fail");
            }
        }
        writeln!(output, "    }}").expect("writing to String cannot fail");
        writeln!(output, "}}").expect("writing to String cannot fail");

        Ok(output)
    }
}

fn register_selector(
    report: &mut ValidationReport,
    selectors: &mut HashMap<String, String>,
    selector: &str,
    locale: &str,
    kind: &str,
) {
    if let Some(previous) = selectors.get(selector) {
        if previous != locale {
            report.push(
                "ambiguous_locale_selector",
                format!(
                    "{kind} `{selector}` for locale `{locale}` conflicts with locale `{previous}`"
                ),
            );
        }
    } else {
        selectors.insert(selector.to_owned(), locale.to_owned());
    }
}

fn read_json<T>(path: &Path) -> Result<T, I18nError>
where
    T: for<'de> Deserialize<'de>,
{
    let contents = fs::read_to_string(path).map_err(|source| I18nError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&contents).map_err(|source| I18nError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn write_json<T>(path: &Path, value: &T) -> Result<(), I18nError>
where
    T: Serialize,
{
    let mut serialized = serde_json::to_string_pretty(value)
        .map_err(|source| I18nError::InvalidArgument(source.to_string()))?;
    serialized.push('\n');
    fs::write(path, serialized).map_err(|source| I18nError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn valid_locale_tag(value: &str) -> bool {
    !value.is_empty()
        && value.split('-').all(|segment| {
            !segment.is_empty()
                && segment
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        })
}

fn valid_language_fallback(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphabetic())
}

fn valid_parameter_name(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some(character) if character.is_ascii_lowercase())
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}

fn rust_keyword(value: &str) -> bool {
    matches!(
        value,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "yield"
    )
}

fn rust_message_identifier(value: &str) -> Option<String> {
    rust_type_identifier(value.strip_prefix("cli.").unwrap_or(value))
}

fn rust_type_identifier(value: &str) -> Option<String> {
    let mut identifier = String::new();
    for segment in value.split(['.', '-', '_']) {
        let mut characters = segment.chars();
        let first = characters.next()?;
        if !first.is_ascii_alphanumeric() {
            return None;
        }
        identifier.push(first.to_ascii_uppercase());
        for character in characters {
            if !character.is_ascii_alphanumeric() {
                return None;
            }
            identifier.push(character.to_ascii_lowercase());
        }
    }
    if identifier.is_empty()
        || identifier
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit())
        || identifier == "Self"
    {
        None
    } else {
        Some(identifier)
    }
}

fn placeholders(text: &str) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut remaining = text;
    while let Some(start) = remaining.find(['{', '}']) {
        let (_, tail) = remaining.split_at(start);
        if tail.starts_with('}') {
            return Err("closing brace without opening brace".to_owned());
        }
        let Some(end) = tail.find('}') else {
            return Err("opening brace without closing brace".to_owned());
        };
        let name = &tail[1..end];
        if !valid_parameter_name(name) || rust_keyword(name) {
            return Err(format!("invalid placeholder `{{{name}}}`"));
        }
        result.push(name.to_owned());
        remaining = &tail[end + 1..];
    }
    Ok(result)
}

fn placeholder_set_matches(text: &str, parameters: &[String]) -> bool {
    let Ok(found) = placeholders(text) else {
        return false;
    };
    let found = found.into_iter().collect::<BTreeSet<_>>();
    let expected = parameters.iter().cloned().collect::<BTreeSet<_>>();
    found == expected
}

fn emit_render_body(output: &mut String, text: &str) -> Result<(), String> {
    let mut cursor = 0;
    let mut chunks = Vec::<(&str, Option<&str>)>::new();
    while let Some(relative_start) = text[cursor..].find('{') {
        let start = cursor + relative_start;
        let end = text[start..]
            .find('}')
            .map(|relative_end| start + relative_end)
            .ok_or_else(|| "opening brace without closing brace".to_owned())?;
        chunks.push((&text[cursor..start], Some(&text[start + 1..end])));
        cursor = end + 1;
    }
    chunks.push((&text[cursor..], None));

    if chunks.len() == 1 {
        writeln!(output, "            String::from({text:?})")
            .expect("writing to String cannot fail");
        return Ok(());
    }

    writeln!(output, "            let mut output = String::new();")
        .expect("writing to String cannot fail");
    for (literal, placeholder) in chunks {
        if !literal.is_empty() {
            let mut characters = literal.chars();
            match (characters.next(), characters.next()) {
                (Some(character), None) => {
                    writeln!(output, "            output.push({character:?});")
                        .expect("writing to String cannot fail");
                }
                _ => {
                    writeln!(output, "            output.push_str({literal:?});")
                        .expect("writing to String cannot fail");
                }
            }
        }
        if let Some(placeholder) = placeholder {
            writeln!(output, "            output.push_str({placeholder});")
                .expect("writing to String cannot fail");
        }
    }
    writeln!(output, "            output").expect("writing to String cannot fail");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{I18nError, add_locale, compile, coverage, emit_render_body, missing, validate};

    static NEXT_WORKSPACE: AtomicU64 = AtomicU64::new(1);

    struct TestWorkspace {
        root: PathBuf,
    }

    impl TestWorkspace {
        fn new() -> Self {
            let id = NEXT_WORKSPACE.fetch_add(1, Ordering::Relaxed);
            let root =
                std::env::temp_dir().join(format!("bulls-i18n-test-{}-{id}", std::process::id()));
            fs::create_dir_all(root.join("locales")).expect("test workspace must be created");
            let workspace = Self { root };
            workspace.write(
                "manifest.json",
                r#"{
  "schema_version": 1,
  "default_locale": "en-US",
  "locales": [
    {"tag": "en-US", "name": "English", "aliases": ["en", "en-US"], "language_fallback": "en"},
    {"tag": "pt-BR", "name": "Português (Brasil)", "aliases": ["pt", "pt-BR"]}
  ]
}"#,
            );
            workspace.write(
                "messages.json",
                r#"{
  "schema_version": 1,
  "messages": {
    "cli.error": {"parameters": []},
    "cli.error.unknown_option": {"parameters": ["option"]}
  }
}"#,
            );
            workspace.write(
                "locales/en-US.json",
                r#"{
  "locale": "en-US",
  "messages": {
    "cli.error": "Error",
    "cli.error.unknown_option": "Unknown option: {option}"
  }
}"#,
            );
            workspace.write(
                "locales/pt-BR.json",
                r#"{
  "locale": "pt-BR",
  "messages": {
    "cli.error": "Erro",
    "cli.error.unknown_option": "Opção desconhecida: {option}"
  }
}"#,
            );
            workspace
        }

        fn path(&self) -> &Path {
            &self.root
        }

        fn write(&self, relative: &str, contents: &str) {
            fs::write(self.root.join(relative), format!("{contents}\n"))
                .expect("test file must be written");
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn assert_issue(workspace: &TestWorkspace, code: &str) {
        let report = validate(workspace.path()).expect("validation must run");
        assert!(
            report.issues().iter().any(|issue| issue.code() == code),
            "expected issue `{code}`, got {:?}",
            report.issues()
        );
    }

    #[test]
    fn render_body_uses_char_push_for_single_character_literals() {
        let mut generated = String::new();

        emit_render_body(&mut generated, "({value}){suffix}。")
            .expect("render body generation must succeed");

        assert!(generated.contains("output.push('(');"));
        assert!(generated.contains("output.push_str(value);"));
        assert!(generated.contains("output.push(')');"));
        assert!(generated.contains("output.push_str(suffix);"));
        assert!(generated.contains("output.push('。');"));
        assert!(!generated.contains("output.push_str(\"(\");"));
        assert!(!generated.contains("output.push_str(\")\");"));
        assert!(!generated.contains("output.push_str(\"。\");"));
    }

    #[test]
    fn valid_catalogs_compile_to_typed_rust() {
        let workspace = TestWorkspace::new();
        let output = workspace.path().join("generated.rs");

        let report = validate(workspace.path()).expect("validation must run");
        assert!(
            report.is_valid(),
            "unexpected issues: {:?}",
            report.issues()
        );
        compile(workspace.path(), &output).expect("valid catalogs must compile");

        let generated = fs::read_to_string(output).expect("generated Rust must exist");
        assert!(generated.contains("pub enum Locale"));
        assert!(generated.contains("pub const DEFAULT_LOCALE: Locale = Locale::EnUs;"));
        assert!(generated.contains("pub fn locale_from_selector"));
        assert!(generated.contains("\"pt-br\" => Some(Locale::PtBr)"));
        assert!(generated.contains("pub fn locale_from_language_fallback"));
        assert!(generated.contains("\"en\" => Some(Locale::EnUs)"));
        assert!(generated.contains("pub enum Message<'a>"));
        assert!(generated.contains("    Error,"));
        assert!(generated.contains("    ErrorUnknownOption {"));
        assert!(generated.contains("option: &'a str"));
    }

    #[test]
    fn manifest_rejects_duplicate_locales_and_ambiguous_aliases() {
        let workspace = TestWorkspace::new();
        workspace.write(
            "manifest.json",
            r#"{
  "schema_version": 1,
  "default_locale": "en-US",
  "locales": [
    {"tag": "en-US", "name": "English", "aliases": ["shared"]},
    {"tag": "EN-us", "name": "English duplicate", "aliases": ["shared"]}
  ]
}"#,
        );

        assert_issue(&workspace, "duplicate_locale");
        assert_issue(&workspace, "ambiguous_locale_selector");
    }

    #[test]
    fn manifest_rejects_invalid_or_ambiguous_language_fallbacks() {
        let workspace = TestWorkspace::new();
        workspace.write(
            "manifest.json",
            r#"{
  "schema_version": 1,
  "default_locale": "en-US",
  "locales": [
    {"tag": "en-US", "name": "English", "aliases": ["en"], "language_fallback": "en-US"},
    {"tag": "pt-BR", "name": "Português (Brasil)", "aliases": ["pt"], "language_fallback": "en"}
  ]
}"#,
        );

        assert_issue(&workspace, "invalid_language_fallback");
        assert_issue(&workspace, "language_fallback_selector_conflict");
    }

    #[test]
    fn manifest_rejects_language_fallback_owned_by_multiple_locales() {
        let workspace = TestWorkspace::new();
        workspace.write(
            "manifest.json",
            r#"{
  "schema_version": 1,
  "default_locale": "en-US",
  "locales": [
    {"tag": "en-US", "name": "English", "aliases": ["en-US"], "language_fallback": "en"},
    {"tag": "pt-BR", "name": "Português (Brasil)", "aliases": ["pt-BR"], "language_fallback": "en"}
  ]
}"#,
        );

        assert_issue(&workspace, "ambiguous_language_fallback");
    }

    #[test]
    fn manifest_requires_registered_default_locale() {
        let workspace = TestWorkspace::new();
        workspace.write(
            "manifest.json",
            r#"{
  "schema_version": 1,
  "default_locale": "de",
  "locales": [
    {"tag": "en-US", "name": "English", "aliases": ["en"]},
    {"tag": "pt-BR", "name": "Português (Brasil)", "aliases": ["pt"]}
  ]
}"#,
        );

        assert_issue(&workspace, "missing_default_locale");
    }

    #[test]
    fn catalog_rejects_missing_unknown_and_empty_messages() {
        let workspace = TestWorkspace::new();
        workspace.write(
            "locales/pt-BR.json",
            r#"{
  "locale": "pt-BR",
  "messages": {
    "cli.error": "",
    "cli.extra": "extra"
  }
}"#,
        );

        assert_issue(&workspace, "empty_translation");
        assert_issue(&workspace, "missing_message");
        assert_issue(&workspace, "unknown_message");
    }

    #[test]
    fn catalog_rejects_placeholder_changes() {
        let workspace = TestWorkspace::new();
        workspace.write(
            "locales/pt-BR.json",
            r#"{
  "locale": "pt-BR",
  "messages": {
    "cli.error": "Erro",
    "cli.error.unknown_option": "Opção desconhecida: {value}"
  }
}"#,
        );

        assert_issue(&workspace, "missing_placeholder");
        assert_issue(&workspace, "additional_placeholder");
    }

    #[test]
    fn catalog_rejects_malformed_placeholders() {
        let workspace = TestWorkspace::new();
        workspace.write(
            "locales/pt-BR.json",
            r#"{
  "locale": "pt-BR",
  "messages": {
    "cli.error": "Erro",
    "cli.error.unknown_option": "Opção desconhecida: {option"
  }
}"#,
        );

        assert_issue(&workspace, "malformed_placeholder");
    }

    #[test]
    fn catalog_rejects_unregistered_files_and_locale_mismatch() {
        let workspace = TestWorkspace::new();
        workspace.write(
            "locales/de.json",
            r#"{
  "locale": "fr",
  "messages": {
    "cli.error": "Fehler",
    "cli.error.unknown_option": "Unbekannte Option: {option}"
  }
}"#,
        );
        workspace.write(
            "locales/pt-BR.json",
            r#"{
  "locale": "pt",
  "messages": {
    "cli.error": "Erro",
    "cli.error.unknown_option": "Opção desconhecida: {option}"
  }
}"#,
        );

        assert_issue(&workspace, "unexpected_catalog");
        assert_issue(&workspace, "catalog_locale_mismatch");
    }

    #[test]
    fn message_contract_rejects_generated_identifier_collisions() {
        let workspace = TestWorkspace::new();
        workspace.write(
            "messages.json",
            r#"{
  "schema_version": 1,
  "messages": {
    "cli.foo_bar": {"parameters": []},
    "cli.foo.bar": {"parameters": []}
  }
}"#,
        );

        assert_issue(&workspace, "message_identifier_collision");
    }

    #[test]
    fn duplicate_json_message_keys_are_rejected_during_parsing() {
        let workspace = TestWorkspace::new();
        workspace.write(
            "messages.json",
            r#"{
  "schema_version": 1,
  "messages": {
    "cli.error": {"parameters": []},
    "cli.error": {"parameters": []}
  }
}"#,
        );

        assert!(matches!(
            validate(workspace.path()),
            Err(I18nError::Json { .. })
        ));
    }

    #[test]
    fn coverage_and_missing_report_translation_state_without_becoming_authority() {
        let workspace = TestWorkspace::new();
        workspace.write(
            "locales/pt-BR.json",
            r#"{
  "locale": "pt-BR",
  "messages": {
    "cli.error": "Erro",
    "cli.error.unknown_option": ""
  }
}"#,
        );

        let coverage = coverage(workspace.path()).expect("coverage must be available");
        assert_eq!(coverage.locales(), ["en-US", "pt-BR"]);
        assert_eq!(coverage.locale_totals(), [2, 1]);

        let missing = missing(workspace.path(), "pt-BR").expect("missing must be available");
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].message(), "cli.error.unknown_option");
        assert_eq!(missing[0].reason(), "empty");
    }

    #[test]
    fn add_locale_registers_catalog_with_explicitly_pending_translations() {
        let workspace = TestWorkspace::new();
        let path = add_locale(workspace.path(), "de").expect("locale must be added");

        assert!(path.ends_with("locales/de.json"));
        let pending = missing(workspace.path(), "de").expect("new locale must be queryable");
        assert_eq!(pending.len(), 2);
        assert!(pending.iter().all(|message| message.reason() == "empty"));
        assert_issue(&workspace, "empty_translation");
    }
}
