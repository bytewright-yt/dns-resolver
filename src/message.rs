use crate::{
    header::{Header, HeaderError, Rcode},
    question::{Question, QuestionParseError},
    reader::Reader,
    record::{Record, RecordParseError},
    writer::Writer,
};

/// A whole DNS message: the header and the sections it describes.
///
/// # Wire format (RFC 1035 §4.1)
///
/// ```text
/// +---------------------+
/// |        Header       |  12 bytes, including a count for each section
/// +---------------------+
/// |       Question      |  QDCOUNT questions
/// +---------------------+
/// |        Answer       |  ANCOUNT records
/// +---------------------+
/// |      Authority      |  NSCOUNT records (not handled yet)
/// +---------------------+
/// |      Additional     |  ARCOUNT records (not handled yet)
/// +---------------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Message {
    header: Header,
    questions: Vec<Question>,
    answers: Vec<Record>,
}

impl Message {
    /// Parse the header, then `qd_count` questions, then `an_count` answers.
    pub(crate) fn parse(reader: &mut Reader) -> Result<Self, MessageError> {
        let header = Header::parse(reader)?;
        // Collecting into a `Result` stops at the first error
        let questions = Result::from_iter((0..header.qd_count()).map(|_| Question::parse(reader)))?;
        let answers = Result::from_iter((0..header.an_count()).map(|_| Record::parse(reader)))?;

        Ok(Self {
            header,
            questions,
            answers,
        })
    }

    /// Build a response to `query`: its questions echoed back, `answers` added,
    /// and the header built last with counts taken from the sections.
    ///
    /// Fails if there are more answers than ANCOUNT (a `u16`) can describe.
    pub(crate) fn response_to(
        query: &Message,
        rcode: Rcode,
        answers: Vec<Record>,
    ) -> Result<Self, TooManyAnswers> {
        let an_count = u16::try_from(answers.len()).map_err(|_| TooManyAnswers(answers.len()))?;
        // Every question in a parsed query is echoed, so its QDCOUNT is still accurate
        let header = Header::response_to(&query.header, rcode, query.header.qd_count(), an_count);

        Ok(Self {
            header,
            questions: query.questions.clone(),
            answers,
        })
    }

    pub(crate) fn write(&self, writer: &mut Writer) {
        self.header.write(writer);
        for question in &self.questions {
            question.write(writer);
        }
        for answer in &self.answers {
            answer.write(writer);
        }
    }

    pub(crate) fn header(&self) -> &Header {
        &self.header
    }

    pub(crate) fn questions(&self) -> &[Question] {
        &self.questions
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum MessageError {
    #[error(transparent)]
    Header(#[from] HeaderError),
    #[error(transparent)]
    Question(#[from] QuestionParseError),
    #[error(transparent)]
    Record(#[from] RecordParseError),
}

/// More answers than fit in a message: ANCOUNT is a `u16`.
#[derive(Debug, thiserror::Error)]
#[error("{0} answers is more than a message can hold (65535)")]
pub(crate) struct TooManyAnswers(usize);

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::{Message, MessageError};
    use crate::{
        header::{Header, Rcode},
        name::Name,
        question::{QClass, Question},
        reader::Reader,
        record::{RData, Record},
        writer::Writer,
    };

    /// `dig @127.0.0.1 -p 5353 youtube.com +tries=1 +timeout=1 +noedns`
    const DIG_QUERY: [u8; 29] = [
        0x3f, 0x00, 0x01, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x79, 0x6f,
        0x75, 0x74, 0x75, 0x62, 0x65, 0x03, 0x63, 0x6f, 0x6d, 0x00, 0x00, 0x01, 0x00, 0x01,
    ];

    /// `youtube.com. 300 IN A 127.0.0.1`
    const ANSWER: [u8; 27] = [
        0x07, 0x79, 0x6f, 0x75, 0x74, 0x75, 0x62, 0x65, 0x03, 0x63, 0x6f, 0x6d, 0x00, 0x00, 0x01,
        0x00, 0x01, 0x00, 0x00, 0x01, 0x2c, 0x00, 0x04, 0x7f, 0x00, 0x00, 0x01,
    ];

    fn parse(input: &[u8]) -> Result<Message, MessageError> {
        Message::parse(&mut Reader::new(input))
    }

    fn write(message: &Message) -> Vec<u8> {
        let mut writer = Writer::new();
        message.write(&mut writer);
        writer.into_bytes()
    }

    fn localhost_answer() -> Record {
        let name = Name::parse(&mut Reader::new(&ANSWER[..13])).unwrap();
        Record::new(name, QClass::IN, 300, RData::A(Ipv4Addr::LOCALHOST))
    }

    #[test]
    fn parses_dig_query() {
        let message = parse(&DIG_QUERY).unwrap();

        assert_eq!(
            message.header,
            Header::parse(&mut Reader::new(&DIG_QUERY[..12])).unwrap()
        );
        assert_eq!(
            message.questions,
            [Question::parse(&mut Reader::new(&DIG_QUERY[12..])).unwrap()]
        );
        assert_eq!(message.answers, []);
    }

    #[test]
    fn accessors_return_parsed_parts() {
        let message = parse(&DIG_QUERY).unwrap();

        assert_eq!(message.header(), &message.header);
        assert_eq!(message.questions(), message.questions.as_slice());
    }

    #[test]
    fn dig_query_round_trips() {
        assert_eq!(write(&parse(&DIG_QUERY).unwrap()), DIG_QUERY);
    }

    #[test]
    fn missing_question_fails() {
        // Header says 1 question, but the packet ends after the header
        assert!(matches!(
            parse(&DIG_QUERY[..12]),
            Err(MessageError::Question(_))
        ));
    }

    #[test]
    fn qd_count_larger_than_questions_fails() {
        // QDCOUNT 2, but only one question follows
        let mut input = DIG_QUERY;
        input[5] = 2;

        assert!(matches!(parse(&input), Err(MessageError::Question(_))));
    }

    #[test]
    fn parses_response_with_answer() {
        // Header: QR | RD, NOERROR, QDCOUNT 1, ANCOUNT 1
        let mut input = vec![
            0x3f, 0x00, 0x81, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        ];
        input.extend_from_slice(&DIG_QUERY[12..]);
        input.extend_from_slice(&ANSWER);

        let message = parse(&input).unwrap();

        assert_eq!(message.answers, [localhost_answer()]);
        assert_eq!(write(&message), input);
    }

    #[test]
    fn missing_answer_fails() {
        // ANCOUNT 1, but the packet ends after the question
        let mut input = DIG_QUERY;
        input[7] = 1;

        assert!(matches!(parse(&input), Err(MessageError::Record(_))));
    }

    #[test]
    fn refused_response_to_dig_query() {
        let query = parse(&DIG_QUERY).unwrap();
        let response = Message::response_to(&query, Rcode::Refused, vec![]).unwrap();

        let mut expected = vec![
            // Same ID, QR | RD, REFUSED, QDCOUNT 1, ANCOUNT 0
            0x3f, 0x00, 0x81, 0x05, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        expected.extend_from_slice(&DIG_QUERY[12..]);
        assert_eq!(write(&response), expected);
    }

    #[test]
    fn answer_response_to_dig_query() {
        let query = parse(&DIG_QUERY).unwrap();
        let response =
            Message::response_to(&query, Rcode::NoError, vec![localhost_answer()]).unwrap();

        let mut expected = vec![
            // Same ID, QR | RD, NOERROR, QDCOUNT 1, ANCOUNT 1
            0x3f, 0x00, 0x81, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        ];
        expected.extend_from_slice(&DIG_QUERY[12..]);
        expected.extend_from_slice(&ANSWER);
        assert_eq!(write(&response), expected);
    }

    #[test]
    fn response_counts_follow_sections() {
        let query = parse(&DIG_QUERY).unwrap();

        for count in 0..3 {
            let answers = vec![localhost_answer(); count];
            let response = Message::response_to(&query, Rcode::NoError, answers).unwrap();

            assert_eq!(response.header.qd_count(), 1);
            assert_eq!(usize::from(response.header.an_count()), count);
            // And on the wire: ANCOUNT is bytes 6..8
            let bytes = write(&response);
            assert_eq!(usize::from(u16::from_be_bytes([bytes[6], bytes[7]])), count);
        }
    }

    #[test]
    fn response_ignores_counts_in_query() {
        // A query claiming answers it doesn't have must not leak those counts
        // into the response: build the query by hand with ANCOUNT 1 and an answer
        let mut input = DIG_QUERY.to_vec();
        input[7] = 1;
        input.extend_from_slice(&ANSWER);
        let query = parse(&input).unwrap();

        let response = Message::response_to(&query, Rcode::Refused, vec![]).unwrap();

        assert_eq!(response.header.an_count(), 0);
        assert_eq!(response.answers, []);
    }
}
