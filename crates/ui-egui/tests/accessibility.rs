fn finding_untagged_content_shows_the_focused_accessibility_results() {
    assert!(h.state_mut().execute("a11y.find_untagged"));
    h.run_steps(2);
    assert_eq!(h.state().right, Some(RightPanel::Accessibility));
    let report = &h.state().a11y.report.as_ref().unwrap().1;
    let active: Vec<Rule> = report.results.iter().filter(|result| result.status != Status::Skipped).map(|result| result.rule).collect();
    assert_eq!(active, [Rule::TaggedContent, Rule::TaggedAnnotations, Rule::TaggedFormFields]);
