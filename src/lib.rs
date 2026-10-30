//! Typed Rust bindings for FpML 5.12 credit default swap confirmations.
//!
//! [`cds::parse`] reads a `creditDefaultSwap` (standalone or inside a full
//! confirmation document) into generated serde types, [`cds::to_xml`] writes
//! it back, and [`cds::key_terms`] pulls out the headline economics.
//! This project is independent and is not endorsed by ISDA.

use quick_xml::{events::Event, Reader, Writer};
use std::io::Cursor;

/// Serde bindings generated from the isolated FpML 5.12 CDS schema.
pub mod cds {
    include!("../generated/fpml_cd.rs");

    /// Deserialize the `creditDefaultSwap` element from a CDS element or full
    /// FpML confirmation document.
    ///
    /// Parsing checks that serialization preserves every XML element in the
    /// input CDS. This catches schema elements that the generated binding
    /// would otherwise silently discard.
    pub fn parse(xml: &str) -> Result<CreditDefaultSwapType, String> {
        let doc = roxmltree::Document::parse(xml).map_err(|error| error.to_string())?;
        let product = doc
            .descendants()
            .find(|node| node.has_tag_name("creditDefaultSwap"))
            .ok_or_else(|| "missing creditDefaultSwap".to_string())?;
        let fragment = &xml[product.range()];
        let mut deserializer = quick_xml::de::Deserializer::from_str(fragment);
        let value: CreditDefaultSwapType = serde_path_to_error::deserialize(&mut deserializer)
            .map_err(|error| format!("{}: {}", error.path(), error.inner()))?;
        let serialized = to_xml(&value)?;
        let input_elements = element_count(fragment)?;
        let output_elements = element_count(&serialized)?;
        if output_elements < input_elements {
            return Err(format!(
                "typed CDS deserialization dropped one or more XML elements ({input_elements} input, {output_elements} serialized)"
            ));
        }
        Ok(value)
    }

    /// Serialize a typed CDS as a standalone `creditDefaultSwap` element.
    pub fn to_xml(value: &CreditDefaultSwapType) -> Result<String, String> {
        quick_xml::se::to_string_with_root("creditDefaultSwap", value)
            .map_err(|error| error.to_string())
    }

    /// Headline economics of a single-name or index CDS.
    #[derive(Debug, Clone, PartialEq, Default)]
    pub struct KeyTerms {
        /// Reference entity name, or the index name for index trades.
        pub reference: Option<String>,
        /// Reference entity RED code, or the index RED pair code.
        pub red: Option<String>,
        /// Protection notional (`protectionTerms/calculationAmount`).
        pub notional: Option<f64>,
        /// Notional currency.
        pub currency: Option<String>,
        /// Fixed coupon as a decimal (0.01 = 100bp).
        pub fixed_rate: Option<f64>,
        /// Unadjusted effective date.
        pub effective_date: Option<String>,
        /// Unadjusted scheduled termination date.
        pub termination_date: Option<String>,
    }

    /// Read the headline terms from a parsed CDS.
    pub fn key_terms(cds: &CreditDefaultSwapType) -> KeyTerms {
        cds.content.iter().fold(KeyTerms::default(), |mut k, item| {
            match item {
                CreditDefaultSwapTypeContent::GeneralTerms(terms) => {
                    terms.content.iter().for_each(|term| match term {
                        GeneralTermsTypeContent::EffectiveDate(d) => {
                            k.effective_date = unadjusted(d)
                        }
                        GeneralTermsTypeContent::ScheduledTerminationDate(d) => {
                            k.termination_date = unadjusted(d)
                        }
                        GeneralTermsTypeContent::ReferenceInformation(info) => {
                            info.content.iter().for_each(|v| {
                                if let ReferenceInformationTypeContent::ReferenceEntity(e) = v {
                                    e.content.iter().for_each(|v| match v {
                                        LegalEntityTypeContent::EntityName(n) => {
                                            k.reference = Some(n.content.clone())
                                        }
                                        LegalEntityTypeContent::EntityId(id)
                                            if id.entity_id_scheme.ends_with("entity-id-RED") =>
                                        {
                                            k.red = Some(id.content.clone())
                                        }
                                        _ => {}
                                    })
                                }
                            })
                        }
                        GeneralTermsTypeContent::IndexReferenceInformation(info) => {
                            info.content.iter().for_each(|v| match v {
                                IndexReferenceInformationTypeContent::IndexName(n) => {
                                    k.reference = Some(n.content.clone())
                                }
                                IndexReferenceInformationTypeContent::IndexId(id) => {
                                    k.red = k.red.take().or_else(|| Some(id.content.clone()))
                                }
                                _ => {}
                            })
                        }
                        _ => {}
                    })
                }
                CreditDefaultSwapTypeContent::ProtectionTerms(terms) => {
                    k.notional = k.notional.or(Some(terms.calculation_amount.amount));
                    k.currency = k
                        .currency
                        .take()
                        .or_else(|| Some(terms.calculation_amount.currency.content.clone()));
                }
                CreditDefaultSwapTypeContent::FeeLeg(leg) => leg.content.iter().for_each(|v| {
                    if let FeeLegTypeContent::PeriodicPayment(p) = v {
                        p.content.iter().for_each(|v| {
                            if let PeriodicPaymentTypeContent::FixedAmountCalculation(c) = v {
                                k.fixed_rate = k.fixed_rate.or(Some(c.fixed_rate.content))
                            }
                        })
                    }
                }),
                _ => {}
            }
            k
        })
    }

    fn unadjusted(date: &AdjustableDate2Type) -> Option<String> {
        date.content.iter().find_map(|item| match item {
            AdjustableDate2TypeContent::UnadjustedDate(v) => Some(v.content.clone()),
            _ => None,
        })
    }

    fn element_count(xml: &str) -> Result<usize, String> {
        let doc = roxmltree::Document::parse(xml).map_err(|error| error.to_string())?;
        Ok(doc
            .descendants()
            .filter(roxmltree::Node::is_element)
            .count())
    }
}

/// Values read from a CDS confirmation without the generated bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdsSummary {
    /// Reference entity RED code, when the example identifies one.
    pub entity_red: Option<String>,
    /// Whether the CDS contains a fee leg.
    pub has_fee_leg: bool,
    /// Whether the CDS contains protection terms.
    pub has_protection_terms: bool,
}

/// Extracts the requested typed fields from an FpML XML document.
pub fn inspect_cds(xml: &str) -> Result<CdsSummary, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let cds = doc
        .descendants()
        .find(|n| n.has_tag_name("creditDefaultSwap"))
        .ok_or_else(|| "missing creditDefaultSwap".to_string())?;
    let general = cds.children().find(|n| n.has_tag_name("generalTerms"));
    let reference_information = general.and_then(|g| {
        g.children()
            .find(|n| n.has_tag_name("referenceInformation"))
    });
    let entity_red = reference_information
        .and_then(|r| r.descendants().find(|n| n.has_tag_name("referenceEntity")))
        .and_then(|e| {
            e.children().find(|n| {
                n.has_tag_name("entityId")
                    && n.attribute("entityIdScheme")
                        .is_some_and(|scheme| scheme.ends_with("entity-id-RED"))
            })
        })
        .and_then(|n| n.text())
        .map(str::to_owned);
    Ok(CdsSummary {
        entity_red,
        has_fee_leg: cds.children().any(|n| n.has_tag_name("feeLeg")),
        has_protection_terms: cds.children().any(|n| n.has_tag_name("protectionTerms")),
    })
}

/// Reads and writes XML events using quick-xml, retaining all input events.
pub fn round_trip(xml: &str) -> Result<String, String> {
    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut buf = Vec::new();
    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| e.to_string())?
        {
            Event::Eof => break,
            event => writer
                .write_event(event.into_owned())
                .map_err(|e| e.to_string())?,
        }
        buf.clear();
    }
    String::from_utf8(writer.into_inner().into_inner()).map_err(|e| e.to_string())
}
