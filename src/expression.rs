use std::{ops::Range, string, usize, vec};
use rand::Rng;

const REPEAT_LIMIT: usize = 1000;

enum Token {
    None,
    Literal(String),
    Repetition(Box<Token>, Range<usize>),
    Choice(Vec<Token>),
    Sequence(Vec<Token>),
}

/// a structure designed to take a `String` and convert it into a regex-like `Expression` to generate inputs for machines
/// # features
/// literals (characters, can be bundled into multiple characters using brackets)
/// *    `a` - matches `char` "a"
/// *    `(ab)` - matches `String` "ab"
/// # choices (picks a literal from a list)
/// *    `[abcd]` - matches either `a`, `b`, `c`, or `d`
/// *    `[ab(cd)]` - matches `a`, `b`, or `cd`
/// # repetitions (repeats a given segment n times)
/// *    `*` - 0 to infinity repetitions of literal
/// *    `+` - 1 to infinity repetitions of literal
/// *    `?` - 0 or 1 repetition of literal
/// *    `{a,b}` - a to b repetitions of literal
/// *    `[ab(cd)]{3,12}` - matches `a`, `b`, or `cd` between 3 and 12 times
struct ExpressionParser {}

impl ExpressionParser {
    fn new() -> Self {
        return ExpressionParser { };
    }

    pub fn produce_token(expression: String) -> Result<Token, String> {
        let parser: ExpressionParser = ExpressionParser::new();
        let mut cleaned_vector_expression: Vec<char> = Self::format_input_string(&parser, expression);
        let output_token: Token = Self::parse_string(&parser, &mut cleaned_vector_expression)?;

        return Ok(output_token);
    }

    fn parse_string(&self, c_vec: &Vec<char>) -> Result<Token, String> {
        let index: usize = 0;
        let result_pair: (Vec<Token>, usize) = self.parse_chars(c_vec, '\0', index)?;

        return Ok(Token::Sequence(result_pair.0));
    }

    /// * parses a given `Vec<char>` segment into a `Token::Choice` - just checks for errors and calls `self.parse_chars`
    fn parse_choice(&self, c_vec: &Vec<char>, mut index: usize) -> Result<(Token, usize), String> {
        if c_vec.len() < 3 {
            return Err("Invalid choice - choices must be at least 3 characters long".to_string())
        }
        if c_vec[index] != '[' {
            return Err("Invalid choice - choices must begin with '['".to_string());
        }
        index += 1;

        let result_pair: (Vec<Token>, usize) = self.parse_chars(c_vec, ']', index)?;
        let token_vec = result_pair.0;
        index = result_pair.1;

        return Ok((Token::Choice(token_vec), index));
    }

    /// * parses a given literal, enclosed by (), and returns a `Token` containing it
    /// * does not support nesting of choices and repetitions - just parses pure literals
    /// * returns a `Token::Literal` containing the enclosed `String`
    fn parse_literal(&self, c_vec: &Vec<char>, mut index: usize) -> Result<(Token, usize), String> {
        if c_vec.len() < 2 {
            return Err("Invalid literal - literals must be at least 2 characters long".to_string())
        }
        let mut next_char: char = c_vec[index];
        if next_char != '(' {
            return Err("Invalid literal - literals must begin with '('".to_string());
        }
        // processes starting character before adding string
        index += 1;
        next_char = c_vec[index];

        let mut literal_vec: Vec<char> = Vec::new();
        while next_char != ')' {
            match next_char {
                '[' | '(' | '{' => {
                    return Err(format!("Invalid character in literal ('{}') at index {} - literals do not support nesting", next_char, index));
                }
                ']' | '}' => {
                    return Err(format!("Invalid character in literal ('{}') at index {} - expected literal end before other closures", next_char, index));
                }
                _ => {
                    literal_vec.push(next_char);
                }
            }

            if !c_vec.is_empty() {
                index += 1;
                next_char = c_vec[index];
            }
            else {
                return Err("Failed to find matching exit char in literal (')')".to_string());
            }
        }
        return Ok((Token::Literal(literal_vec.into_iter().collect()), index));
    }

    /// * generic parsing function. processes a given `Vec<char>` considering the next character
    /// * returns a `Vec<token>`, containing all `Token`s at this 'level' of the tree
    fn parse_chars<'a>(&self, c_vec: &Vec<char>, exit_char: char, mut index: usize) -> Result<(Vec<Token>, usize), String> {
        let mut token_vec: Vec<Token> = Vec::new();
        let mut next_char: char = c_vec[index];

        while next_char != exit_char {
            match next_char {
                '[' => {
                    let result_pair: (Token, usize) = self.parse_choice(c_vec, index)?;
                    token_vec.push(result_pair.0);
                    index = result_pair.1;
                }
                '(' => {
                    let result_pair: (Token, usize) = self.parse_literal(c_vec, index)?;
                    token_vec.push(result_pair.0);
                    index = result_pair.1;
                }
                '{' | '*' | '+' | '?' => {
                    if token_vec.is_empty() {
                        return Err(format!("Invalid bracketing at index {} - cannot repeat empty statement", index));
                    }
                    let repeated_token: Token = token_vec.pop().unwrap();
                    let result_pair: (Token, usize) = self.parse_repetition(repeated_token, c_vec, index)?;
                    token_vec.push(result_pair.0);
                    index = result_pair.1;
                }
                ']' | ')' | '}' => {
                    return Err(format!("Invalid bracketing configuration at index {} - recieved unmatched {}", index, next_char));
                }
                _ => {
                    token_vec.push(Token::Literal(next_char.to_string()));
                }
            }
            if !c_vec.is_empty() {
                index += 1;
                next_char = c_vec[index];
            }
            else if exit_char != '\0' {
                return Err(format!("Failed to find expected exit char ('{}')", exit_char));
            }
            else {
                break;
            }
        }

        return Ok((token_vec, index));
    }

    /// * produces a `Range<usize>` from a discrete repetition (bounded by {}, like {4,12})
    /// * returns this range alongside the number of characters used in the repetition
    fn parse_ranged_repetition(&self, c_vec: &Vec<char>, mut index: usize) -> Result<(Range<usize>, usize), String> {
        let mut string_ranges: (String, String) = ("-".to_string(), "-".to_string());
        let accepted_chars: Vec<char> = vec!['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];
        let mut modify_first: bool = true;
        let mut next_char: char = c_vec[index];
        let mut char_range: Vec<char> = Vec::new();
        let start_index: usize = index.clone();

        while next_char != '}' {
            if next_char == ',' && modify_first {
                modify_first = false;
                string_ranges.0 = char_range.into_iter().collect();
                char_range = Vec::new();
            }
            else if accepted_chars.contains(&next_char) {
                char_range.push(next_char);
            }
            else {
                return Err(format!("Repetition parser received invalid character ('{}') at index {}, aborting parsing", next_char, index));
            }

            index += 1;
            next_char = c_vec[index];
        }

        let (start, end) = (string_ranges.0.parse::<usize>().unwrap(), string_ranges.1.parse::<usize>().unwrap());
        if start > end {
            return Err(format!("Received invalid range ({} > {}) from indices {} to {}, aborting parsing", start, end, start_index, index));
        }

        return Ok((start.min(REPEAT_LIMIT)..end.min(REPEAT_LIMIT), index))
    }

    /// * produces a `Token::Repetition` from a generic repetition (either special chars (*, +, ?), or a discrete repetition bounded by {})
    /// * processes a given `Vec<char>` to construct this repetition, removing used characters
    fn parse_repetition<'a>(&self, token: Token, c_vec: &Vec<char>, mut index: usize) -> Result<(Token, usize), String> {
        let custom_range: Range<usize>;
        let first_char: char = c_vec[index];
        index += 1;

        // separates discrete repetitions from symbol-based repetitions
        let range: Range<usize> = match first_char {
            '{' => {
                (custom_range, index) = self.parse_ranged_repetition(c_vec, index)?;
                custom_range
            },
            '*' => 0..REPEAT_LIMIT,
            '+' => 1..REPEAT_LIMIT,
            '?' => 0..1,
            _ => return Err(format!("Unexpected repetition start char reached ('{}') at index {}", first_char, index)),
        };
        // removes processed characters from the char vector, returning the unprocessed remainder of the vector
        return Ok((Token::Repetition(Box::new(token), range), index));
    }

    fn format_input_string(&self, c_string: String) -> Vec<char> {
        return c_string.replace(" ", "").replace("  ", "").chars().collect();
    }
}

struct Expression { 
    token: Token,
    min_length: usize,
}

impl Expression {
    pub fn from_token(&self, expression_token: Token) -> Expression {
        let min_gen_length: usize = Self::calculate_min_length(&expression_token);
        return Expression { token: expression_token, min_length: min_gen_length };
    }
    pub fn from_string(&self, expression_string: String) -> Result<Expression, String> {
        let expression_token: Token = ExpressionParser::produce_token(expression_string)?;
        let min_gen_length: usize = Self::calculate_min_length(&expression_token);
        return Ok(Expression { token: expression_token, min_length: min_gen_length });
    }

    pub fn gen_to_length(&self, length: usize) -> Vec<String> {
        const SAMPLES: usize = 100;
        let mut generated_strings: Vec<String> = Vec::new();

        for _ in 0..SAMPLES {
            let new_string: Option<String> = self.recur_to_length(&self.token, length, SAMPLES);
            match new_string {
                Some(string) => generated_strings.push(string),
                None => {},
            }
        }
        return generated_strings;
    }

    pub fn recur_to_length(&self, expression_token: &Token, target_length: usize, target_samples: usize) -> Option<String> {
        let max_length: usize = Self::calculate_max_length(expression_token);
        let min_length: usize = Self::calculate_min_length(expression_token);

        // if the max length reachable is below the target, or the min length is above, fail and exit
        if max_length < target_length || min_length > target_length {
            return None;
        }
        // if target length has elapsed, exit out successfully
        if target_length == 0 {
            return Some("".to_string());
        }

        // TODO: finish - implement helper function for random constrained integer composition - for use in distributing length across a sequence
        match expression_token {
            Token::Literal(literal) => {
                return Some(literal.to_string());
            }
            Token::Repetition(token, repetitions) => {
                let remaining_repetitions: Range<usize> = repetitions.clone();
                return None;
            }
            Token::Choice(tokens) => {
                return None;
            }
            Token::Sequence(tokens) => {
                return None;
            }
            Token::None => {
                return None;
            }
        }
    }

    pub fn gen_samples(&self, samples: usize) -> Vec<String> {
        return vec![];
    }

    // biased simple partition producer
    fn produce_partitions(random: &mut impl Rng, lower: &[usize], upper: &[usize], target_length: usize) -> Option<Vec<usize>> {
        let min_lower: usize = lower.iter().sum();
        let max_upper: usize = upper.iter().sum();
        let partitions: usize = lower.len();
        let mut lengths: Vec<usize> = Vec::with_capacity(partitions);

        if target_length < min_lower || target_length > max_upper {
            return None;
        }

        let mut remaining_allocation: usize = target_length - min_lower;
        // implements random bounded composition to find valid random allocations of size to each partition
        for i in 0..partitions {
            let lower_partiton: usize = lower[i];
            let upper_partition: usize = upper[i];
            let max_allocation: usize = upper_partition - lower_partiton;
            let allocation: usize;

            // if this is the final allocation, allocate the remainder of length to this partition
            if i == partitions - 1 {
                allocation = remaining_allocation;
            }
            // otherwise, allocate a random amount of length between 0 and the remaining allocation/max allocation to it
            else {
                allocation = random.random_range(0..=remaining_allocation.min(max_allocation));
            }
            lengths.push(lower_partiton + allocation);
            remaining_allocation -= allocation;
        }

        return Some(lengths);
    }

    fn calculate_min_length(analysed_token: &Token) -> usize {
        return match analysed_token {
            Token::Literal(literal) => literal.len(),
            Token::Repetition(repeated_token, repetitions) => Self::calculate_min_length(repeated_token.as_ref()) * repetitions.start,
            Token::Choice(token_vec) => token_vec.iter().map(| token | Self::calculate_min_length(token)).min().unwrap_or(0),
            Token::Sequence(token_vec) => token_vec.iter().map(| token | Self::calculate_min_length(token)).sum(),
            Token::None => 0,
        };
    }

    fn calculate_max_length(analysed_token: &Token) -> usize {
        return match analysed_token {
            Token::Literal(literal) => literal.len(),
            Token::Repetition(repeated_token, repetitions) => Self::calculate_max_length(repeated_token.as_ref()) * repetitions.end,
            Token::Choice(token_vec) => token_vec.iter().map(| token | Self::calculate_max_length(token)).max().unwrap_or(0),
            Token::Sequence(token_vec) => token_vec.iter().map(| token | Self::calculate_min_length(token)).sum(),
            Token::None => 0,
        }
    }

    // overflow-resilient summing function
    fn overflow_sum<I>(iter: I) -> usize where I: IntoIterator<Item = usize> {
        let mut sum: usize = 0;
        for value in iter {
            match sum.checked_add(value) {
                Some(value) => sum = value,
                None => return usize::MAX,
            }
        }
        return sum;
    }
}