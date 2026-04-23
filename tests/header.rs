use niiterm::modality::Modality;

#[test]
fn modality_defaults_are_stable() {
    assert_eq!(Modality::T1.label(), "T1w");
    assert_eq!(Modality::Dwi.label(), "DWI");
}
