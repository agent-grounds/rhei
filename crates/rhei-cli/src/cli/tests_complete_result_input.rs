// Unit tests for alternate completion-result source loading.

mod complete_result_input_tests {
    use super::super::*;
    use std::io::{self, Read};

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::Other, "injected read failure"))
        }
    }

    /// A source read error must be reported rather than accepting partial input.
    // §FS-rhei-complete.2.2 §FS-rhei-complete.4
    #[test]
    fn reports_an_injected_result_reader_error() {
        let error = read_complete_result(FailingReader, "injected standard input")
            .expect_err("an injected reader error must fail");
        let rendered = error.to_string();
        assert!(
            rendered.contains(
                "failed to read the completion result from injected standard input"
            ),
            "missing source context: {rendered}"
        );
        assert!(rendered.contains("injected read failure"), "missing read error: {rendered}");
    }
}
