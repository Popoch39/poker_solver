use nitro_solver::HandClass;

#[test]
fn there_are_169_hand_classes_covering_1326_combos() {
    let classes = HandClass::all();
    assert_eq!(classes.count(), 169);
    let combos: u32 = HandClass::all().map(HandClass::combos).sum();
    assert_eq!(combos, 1326);
}

#[test]
fn parses_and_prints_standard_notation() {
    for text in ["AA", "AKs", "K7o", "32o", "T9s", "22"] {
        let hand: HandClass = text.parse().unwrap();
        assert_eq!(hand.to_string(), text);
    }
    assert_eq!("7Ko".parse::<HandClass>().unwrap().to_string(), "K7o");
    assert_eq!("ak".parse::<HandClass>().ok(), None);
    assert_eq!("AAs".parse::<HandClass>().ok(), None);
    assert_eq!("AKx".parse::<HandClass>().ok(), None);
    assert_eq!("".parse::<HandClass>().ok(), None);
}

#[test]
fn combos_depend_on_pair_suited_or_offsuit() {
    let combos = |s: &str| s.parse::<HandClass>().unwrap().combos();
    assert_eq!(combos("QQ"), 6);
    assert_eq!(combos("AKs"), 4);
    assert_eq!(combos("AKo"), 12);
}

#[test]
fn grid_places_pairs_on_the_diagonal_and_suited_hands_above_it() {
    let at = |row, col| HandClass::at_grid(row, col).to_string();
    assert_eq!(at(0, 0), "AA");
    assert_eq!(at(0, 1), "AKs");
    assert_eq!(at(1, 0), "AKo");
    assert_eq!(at(12, 12), "22");
    assert_eq!(at(6, 3), "J8o");
    assert_eq!(at(3, 6), "J8s");
    for hand in HandClass::all() {
        let (row, col) = hand.grid_position();
        assert_eq!(HandClass::at_grid(row, col), hand);
    }
}
