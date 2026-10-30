use fpml::{inspect_cds, round_trip};
use std::{fs, path::Path};

fn example_files() -> Vec<std::path::PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("schemas/confirmation/examples/credit-derivatives");
    let mut paths: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(Result::unwrap)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("cd-ex"))
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    paths
}

#[test]
fn cds_examples_round_trip_and_expose_confirmation_fields() {
    let examples = example_files();
    assert_eq!(examples.len(), 24);
    let mut red_ids = 0;
    for path in examples {
        let input = fs::read_to_string(&path).unwrap();
        let summary = inspect_cds(&input).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(summary.has_fee_leg, "{}", path.display());
        assert!(summary.has_protection_terms, "{}", path.display());
        red_ids += usize::from(summary.entity_red.is_some());
        let output = round_trip(&input).unwrap();
        let round_trip_summary = inspect_cds(&output).unwrap();
        assert_eq!(summary, round_trip_summary, "{}", path.display());
    }
    assert!(
        red_ids >= 20,
        "expected RED identifiers across the CDS examples"
    );
}

#[test]
fn generated_cds_types_carry_data_and_round_trip_as_typed_values() {
    let examples = example_files();
    assert_eq!(examples.len(), 24);
    for path in &examples {
        let input = fs::read_to_string(path).unwrap();
        let typed =
            fpml::cds::parse(&input).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let xml = fpml::cds::to_xml(&typed).unwrap();
        let reparsed = fpml::cds::parse(&xml)
            .unwrap_or_else(|error| panic!("{} serialized: {error}\n{xml}", path.display()));
        assert_eq!(typed, reparsed, "{}", path.display());
    }

    // This example makes the concrete values observable rather than merely
    // checking that a generated Rust type can be constructed.
    let input = fs::read_to_string(&examples[0]).unwrap();
    let typed = fpml::cds::parse(&input).unwrap();
    let fpml::cds::CreditDefaultSwapType { content, .. } = typed;
    let mut red_id = None;
    let mut effective_date = None;
    let mut termination_date = None;
    let mut notional = None;
    let mut notional_currency = None;
    let mut fixed_rate = None;
    for item in content {
        match item {
            fpml::cds::CreditDefaultSwapTypeContent::GeneralTerms(terms) => {
                for term in terms.content {
                    match term {
                        fpml::cds::GeneralTermsTypeContent::EffectiveDate(date) => {
                            effective_date = date_value(date)
                        }
                        fpml::cds::GeneralTermsTypeContent::ScheduledTerminationDate(date) => {
                            termination_date = date_value(date)
                        }
                        fpml::cds::GeneralTermsTypeContent::ReferenceInformation(info) => {
                            for value in info.content {
                                if let fpml::cds::ReferenceInformationTypeContent::ReferenceEntity(
                                    entity,
                                ) = value
                                {
                                    for value in entity.content {
                                        if let fpml::cds::LegalEntityTypeContent::EntityId(id) =
                                            value
                                        {
                                            if id.entity_id_scheme.ends_with("entity-id-RED") {
                                                red_id = Some(id.content);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            fpml::cds::CreditDefaultSwapTypeContent::ProtectionTerms(terms) => {
                notional = Some(terms.calculation_amount.amount);
                notional_currency = Some(terms.calculation_amount.currency);
            }
            fpml::cds::CreditDefaultSwapTypeContent::FeeLeg(leg) => {
                for payment in leg.content {
                    if let fpml::cds::FeeLegTypeContent::PeriodicPayment(payment) = payment {
                        for item in payment.content {
                            if let fpml::cds::PeriodicPaymentTypeContent::FixedAmountCalculation(
                                calc,
                            ) = item
                            {
                                fixed_rate = Some(calc.fixed_rate.content);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    assert_eq!(red_id.as_deref(), Some("004CC9"));
    assert_eq!(effective_date.as_deref(), Some("2002-12-05"));
    assert_eq!(termination_date.as_deref(), Some("2007-12-05"));
    assert_eq!(notional, Some(500000000.0));
    assert_eq!(
        notional_currency
            .as_ref()
            .map(|currency| currency.content.as_str()),
        Some("JPY")
    );
    assert_eq!(fixed_rate, Some(0.007));
}

#[test]
fn typed_parser_rejects_elements_the_binding_would_drop() {
    let input = fs::read_to_string(&example_files()[0]).unwrap();
    let input = input.replacen(
        "<calculationAmount>",
        "<calculationAmount><unrecognizedCdsElement/>",
        1,
    );
    let error = fpml::cds::parse(&input).unwrap_err();
    assert!(
        error.contains("dropped one or more XML elements"),
        "{error}"
    );
}

fn date_value(date: fpml::cds::AdjustableDate2Type) -> Option<String> {
    date.content.into_iter().find_map(|item| match item {
        fpml::cds::AdjustableDate2TypeContent::UnadjustedDate(value) => Some(value.content),
        _ => None,
    })
}

#[test]
fn key_terms_read_headline_economics() {
    let input = fs::read_to_string(&example_files()[0]).unwrap();
    let k = fpml::cds::key_terms(&fpml::cds::parse(&input).unwrap());
    assert_eq!(k.red.as_deref(), Some("004CC9"));
    assert_eq!(k.notional, Some(500000000.0));
    assert_eq!(k.currency.as_deref(), Some("JPY"));
    assert_eq!(k.fixed_rate, Some(0.007));
    assert_eq!(k.effective_date.as_deref(), Some("2002-12-05"));
    assert_eq!(k.termination_date.as_deref(), Some("2007-12-05"));
    assert!(k.reference.is_some());
    let all: Vec<_> = example_files()
        .iter()
        .map(|p| fpml::cds::key_terms(&fpml::cds::parse(&fs::read_to_string(p).unwrap()).unwrap()))
        .collect();
    assert!(all
        .iter()
        .all(|k| k.notional.is_some() && k.currency.is_some()));
    assert!(all.iter().filter(|k| k.reference.is_some()).count() >= 20);
}

#[test]
fn every_credit_derivatives_example_parses() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("schemas/confirmation/examples/credit-derivatives");
    let paths: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "xml"))
        .collect();
    assert_eq!(paths.len(), 44);
    for path in paths {
        fpml::cds::parse(&fs::read_to_string(&path).unwrap())
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }
}
