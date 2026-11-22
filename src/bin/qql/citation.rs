// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

use qql::{Record, Source};

#[derive(Clone, Copy)]
pub(super) enum Style {
    Plain,
    Apa,
    Chicago,
    Arabic,
    Markdown,
}

impl Style {
    pub(super) fn named(name: &str) -> Option<Self> {
        match name {
            "apa" => Some(Self::Apa),
            "chicago" => Some(Self::Chicago),
            "arabic" => Some(Self::Arabic),
            "markdown" => Some(Self::Markdown),
            _ => None,
        }
    }

    pub(super) fn render(self, record: &Record, source: &dyn Source) -> String {
        let arabic = matches!(self, Self::Arabic);
        let (preferred, fallback) = if arabic {
            (&record.ar, &record.en)
        } else {
            (&record.en, &record.ar)
        };
        let text = if preferred.is_empty() {
            fallback
        } else {
            preferred
        };
        let (primary_key, item_key) = source.citation_keys();
        let mut locator = locator(record, primary_key, item_key);
        let field = |key: &str| record.extra.get(key).and_then(|v| v.as_str());
        let quran = item_key == "ayah";
        let book = if source.has_books() {
            field(if arabic {
                "book_name_ar"
            } else {
                "book_name_en"
            })
        } else {
            None
        };
        let title = if arabic && quran {
            field("surah_name_ar")
        } else {
            book
        }
        .unwrap_or(&record.collection);

        if arabic {
            locator = arabic_digits(&locator);
        }
        let reference = format!("{title} {locator}");
        match self {
            Self::Plain => format!("{text} — {reference}"),
            Self::Apa => format!(
                "“{text}” ({}, n.d., {}).",
                record.collection,
                if book.is_some() { &reference } else { &locator }
            ),
            Self::Chicago => format!("“{text}” ({reference})."),
            Self::Arabic if quran => format!("﴿{text}﴾ [{reference}]"),
            Self::Arabic => format!("«{text}» [{reference}]"),
            Self::Markdown => format!("> {}\n\n— {reference}", text.replace('\n', "\n> ")),
        }
    }
}

fn locator(record: &Record, primary_key: &str, item_key: &str) -> String {
    let number = |key: &str| record.extra.get(key).and_then(|v| v.as_u64());
    let flat = item_key == "number"
        && record.extra.get("numbering").and_then(|v| v.as_str()) == Some("book");
    match (number(primary_key), number(item_key)) {
        (Some(primary), Some(item)) if !flat => format!("{primary}:{item}"),
        (_, Some(item)) => item.to_string(),
        _ => String::new(),
    }
}

fn arabic_digits(locator: &str) -> String {
    const DIGITS: [char; 10] = ['٠', '١', '٢', '٣', '٤', '٥', '٦', '٧', '٨', '٩'];
    locator
        .chars()
        .map(|c| match c.to_digit(10) {
            Some(n) => DIGITS[n as usize],
            None => c,
        })
        .collect()
}
