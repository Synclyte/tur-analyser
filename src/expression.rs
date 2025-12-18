use std::{collections::HashMap, fmt::format, ops::{Index, Range}, usize, vec};
use rand::{Rng, rng, seq::IndexedRandom};

const REPEAT_LIMIT: usize = 256;

enum Token {
    Literal(String),
    Repetition(Box<Token>, Bound, Bound),
    Choice(Vec<Token>),
    Sequence(Vec<Token>),
}

struct ContextToken {
    token: Token,
    context: HashMap<String, usize>,
}

#[derive(Clone)]
enum Operation {
    Add,
    Subtract,
    Multiply,
}

enum Bound {
    Literal(usize),
    Variable(String),
    Calculation(Box<Bound>, Operation, Box<Bound>)
}

impl Clone for Bound {
    fn clone(&self) -> Self {
        return match &self {
            Self::Literal(size) => Self::Literal(size.clone()),
            Self::Variable(var) => Self::Variable(var.clone()),
            Self::Calculation(bound_1, operator, bound_2) => Self::Calculation(bound_1.clone(), operator.clone(), bound_2.clone())
        }
    }
}

impl Bound {
    fn validate_bound(bound: &Bound, context: &HashMap<String, usize>) -> bool {
        return match bound {
            Bound::Literal(_) => true,
            Bound::Variable(value) => context.get(value).is_some(),
            Bound::Calculation(_, _, _) => Bound::calculate_bound(bound, context) >= 0,
        }
    }

    fn calculate_bound(bound: &Bound, context: &HashMap<String, usize>) -> usize {
        return match bound {
            Bound::Literal(value) => *value,
            Bound::Variable(value) => *context.get(value).unwrap_or(&0),
            Bound::Calculation(first, operator, last) => {
                let first_result: usize = Bound::calculate_bound(first.as_ref(), context);
                let last_result: usize = Bound::calculate_bound(last.as_ref(), context);
                match operator {
                    Operation::Add => (first_result + last_result).min(REPEAT_LIMIT),
                    Operation::Multiply => (first_result * last_result).min(REPEAT_LIMIT),
                    Operation::Subtract => first_result - last_result,
                }
            }
        } 
    }

    fn get_string(bound: &Bound) -> String {
        return match bound {
            Bound::Literal(lit) => lit.to_string(),
            Bound::Variable(var) => var.to_string(),
            Bound::Calculation(first_expression, operation, last_expression) => {
                let operation_string: &str = match operation {
                    Operation::Add => " + ",
                    Operation::Multiply => " * ",
                    Operation::Subtract => " - ",
                };
                return Bound::get_string(first_expression.as_ref()) + operation_string + &Bound::get_string(last_expression.as_ref());
            }
        }
    }
}

/// a structure designed to take a `String` and convert it into a regex-like `Expression` to generate inputs for machines
/// # features
/// literals (characters, can be bundled into multiple characters using brackets)
/// *    `a` - matches `char` "a"
/// *    `(ab)` - matches `String` "ab"
/// # choices (picks a literal from a list)
/// *    `a|b|c|d` - matches either `a`, `b`, `c`, or `d`
/// *    `a|b|cd` - matches `a`, `b`, or `cd`
/// # repetitions (repeats a given segment n times)
/// *    `*` - 0 to infinity repetitions of literal
/// *    `+` - 1 to infinity repetitions of literal
/// *    `?` - 0 or 1 repetition of literal
/// *    `{a,b}` - a to b repetitions of literal
/// *    `(ab|cd){3,12}` - matches `ab`, or `cd` between 3 and 12 times
/// *    `(ab|cd){1+a,2*b-3}` - matches `ab`, or `cd` between 1+a and 2*b-3 times
struct ExpressionParser {
    context: HashMap<String, usize>,
}

impl ExpressionParser {
    fn new() -> Self {
        return ExpressionParser { context: HashMap::new() };
    }

    pub fn produce_token(expression: String) -> Result<ContextToken, String> {
        let mut parser: ExpressionParser = ExpressionParser::new();
        let mut cleaned_vector_expression: Vec<char> = Self::format_input_string(&parser, expression);
        let output_token: Token = Self::parse_string(&mut parser, &mut cleaned_vector_expression)?;

        return Ok(ContextToken{token: output_token, context: parser.context});
    }

    fn parse_string(&mut self, c_vec: &Vec<char>) -> Result<Token, String> {
        let index: usize = 0;
        let result_pair: (Token, usize) = self.parse_chars(c_vec, '\0', index)?;

        return Ok(result_pair.0);
    }

    /// checks for errors in a given literal, and returns a Token::Literal containing the given `Vec<char>` if none are found
    fn parse_literal(&self, c_vec: Vec<char>, index: usize) -> Result<(Token, usize), String> {
        let min_length: usize = 1;
        let initial_length: usize = c_vec.len();
        if initial_length < min_length {
            return Err(format!("Error: Received invalid literal - literals must be at least {} character(s) long", min_length))
        }

        for next_char in &c_vec {
            match next_char {
                '[' | '(' | '{' => {
                    return Err(format!("Error: Received invalid character in literal ('{}') at index {} - literals do not support nesting", next_char, index));
                }
                ']' | ')' | '}'=> {
                    return Err(format!("Error: Received invalid character in literal ('{}') at index {} - invalid closure", next_char, index));
                }
                _ => {}
            }
        }
        return Ok((Token::Literal(c_vec.iter().collect()), index + initial_length));
    }

    /// * generic parsing function. processes a given `Vec<char>` considering the next character
    /// * returns a `Token::Choice` or a `Token::Sequence`, containing all `Token`s at this 'level' of the tree
    fn parse_chars(&mut self, c_vec: &Vec<char>, exit_char: char, mut index: usize) -> Result<(Token, usize), String> {
        let special_chars: Vec<char> = vec!['(', '|', '{', '*', '+', '?', ')', '}', exit_char];
        let total_len: usize = c_vec.len();
        let mut token_vec: Vec<Token> = Vec::new();
        let mut next_char: char = c_vec[index];
        let mut literal_buffer: Vec<char> = Vec::new();
        let mut choice_indices: Vec<usize> = vec![0];

        while next_char != exit_char {
            match next_char {
                // handles choices - produces a list containing the indices of the starting position of all choices for later splitting
                '|' => {
                    let next_index: usize = token_vec.len();
                    if token_vec.is_empty() || (choice_indices.last().unwrap() == &next_index) {
                        return Err(format!("Error: Received invalid choice at index {} - choices must split input", index));
                    }
                    choice_indices.push(next_index);
                }
                // handles sequences
                '(' => {
                    let result_pair: (Token, usize) = self.parse_chars(c_vec, ')', index + 1)?;
                    token_vec.push(result_pair.0);
                    index = result_pair.1;
                }
                // handles repetitions
                '{' | '*' | '+' | '?' => {
                    if token_vec.is_empty() {
                        return Err(format!("Error: Received invalid repetition configuration at index {} - cannot repeat empty statement", index));
                    }
                    let repeated_token: Token = token_vec.pop().unwrap();
                    let result_pair: (Token, usize) = self.parse_repetition(repeated_token, c_vec, index)?;
                    token_vec.push(result_pair.0);
                    index = result_pair.1;
                }
                // handles invalid bracketing - valid bracketing handled at the start of the loop
                ']' | ')' | '}' => {
                    return Err(format!("Error: Received invalid bracketing configuration at index {} - received unmatched {}", index, next_char));
                }
                _ => {
                    literal_buffer.push(next_char);
                }
            }
            index += 1;
            let exceeded_length: bool = index >= total_len;

            if !exceeded_length {
                next_char = c_vec[index];
            }
            // produces a literal if the next character would not continue the literal
            // positioning here ensures that this always executes before exiting
            if (special_chars.contains(&next_char) || exceeded_length) && !literal_buffer.is_empty() {
                let result_pair: (Token, usize) = self.parse_literal(literal_buffer, index)?;
                literal_buffer = Vec::new();
                token_vec.push(result_pair.0);
            }

            if exceeded_length {
                if exit_char != '\0' {
                    return Err(format!("Error: Failed to find expected exit char ('{}') in sequence", exit_char));
                }
                else {
                    break;
                }
            }
        }

        // if this sequence contains a choice, then the entire block is a choice
        if choice_indices.len() > 1 {
            let mut first: usize;
            let mut last: usize = token_vec.len();
            let mut choice_vec: Vec<Token> = Vec::new();

            // iterates through contained tokens and produces a single token for each choice
            for n in (0..choice_indices.len()).rev() {
                first = choice_indices[n];

                let mut token_slice: Vec<Token> = token_vec.drain(first..last).collect();
                choice_vec.push(match token_slice.len() {
                    1 => token_slice.pop().unwrap(),
                    _ => Token::Sequence(token_slice),
                });

                last = first;
            }
            choice_vec.reverse();

            return Ok((Token::Choice(choice_vec), index));
        }

        return Ok((Token::Sequence(token_vec), index));
    }

    /// * produces a `Range<usize>` from a discrete repetition (bounded by {}, like {4,12})
    /// * returns this range alongside the number of characters used in the repetition
    fn parse_ranged_repetition(&mut self, c_vec: &Vec<char>, mut index: usize) -> Result<((Bound, Bound), usize), String> {
        let end_index: usize;
        let retrieved_end_index: Option<usize> = c_vec.iter().enumerate().skip(index + 1).find(| &(i, &c) | c == '}').map(|(i, _)| i);
        match retrieved_end_index {
            None => return Err(format!("Error: Failed to find end of repetition starting at index {}", index)),
            Some(found_index) => end_index = found_index,
        }
        let extracted_range: String = c_vec[index..end_index].to_vec().into_iter().collect();

        let mut split_range = extracted_range.split(',');
        let first: String;
        let last: String;
        match extracted_range.contains(',') {
            true => {
                first = split_range.next().unwrap().to_string();
                last = split_range.next().unwrap().to_string();
            }
            false => return Err(format!("Error: Failed to find range split in repetition starting at index {}", index)),
        }

        let lower_bound: Bound = if first.is_empty() {
            Bound::Literal(0)
        }
        else {
            self.parse_arithmetic_expression(&first)?
        };

        let upper_bound: Bound = if last.is_empty() {
            Bound::Literal(REPEAT_LIMIT)
        }
        else {
            self.parse_arithmetic_expression(&last)?
        };

        return Ok(((lower_bound, upper_bound), end_index));
    }

    /// helper method for parse_arithmetic_expression - creates a Bound::Calculation from a string containing an operation
    fn get_calculation(&mut self, expression: &String, split_index: usize, operation_type: Operation) -> Result<Bound, String> {
        let (first, remainder) = expression.split_at(split_index);
        let last = &remainder[1..];

        return Ok(Bound::Calculation(
            Box::new(self.parse_arithmetic_expression(&first.to_string())?),
            operation_type,
            Box::new(self.parse_arithmetic_expression(&last.to_string())?)
        ))
    }

    // left to right, multiplication then add/subtract
    fn parse_arithmetic_expression(&mut self, expression: &String) -> Result<Bound, String> {
        if expression.is_empty() {
            return Err("Error: Range expression is invalid - operators must have two arguments".to_string());
        }

        let mult_location: usize = expression.find("*").unwrap_or_else(|| usize::MAX);
        let add_location: usize = expression.find("+").unwrap_or_else(|| usize::MAX);
        let sub_location: usize = expression.find("-").unwrap_or_else(|| usize::MAX);

        // handles operations
        if mult_location != usize::MAX {
            return self.get_calculation(expression, mult_location, Operation::Multiply)
        }
        else if add_location != usize::MAX || sub_location != usize::MAX {
            if sub_location <= add_location {
                return self.get_calculation(expression, sub_location, Operation::Subtract);
            }
            else {
                return self.get_calculation(expression, add_location, Operation::Add);
            }
        }
        
        // handles variables
        if expression.len() == 1 {
            let first_char = expression.chars().nth(0).unwrap();
            if first_char >= 'a' && first_char <= 'z' {
                let char_string: String = first_char.to_string();
                self.context.insert(char_string.clone(), 0);
                return Ok(Bound::Variable(char_string));
            }
        }
        // handles literals
        let parsed_literal = expression.parse::<usize>();
        return match parsed_literal {
            Ok(literal) => Ok(Bound::Literal(literal)),
            Err(_) => Err(format!("Error: Range literal ('{}') could not be parsed", expression)),
        }
    }

    /// * produces a `Token::Repetition` from a generic repetition (either special chars (*, +, ?), or a discrete repetition bounded by {})
    /// * processes a given `Vec<char>` to construct this repetition, removing used characters
    fn parse_repetition(&mut self, token: Token, c_vec: &Vec<char>, mut index: usize) -> Result<(Token, usize), String> {
        let first_char: char = c_vec[index];
        index += 1;

        // separates discrete repetitions from symbol-based repetitions
        let custom_range: (Bound, Bound);
        let range: (Bound, Bound) = match first_char {
            '{' => {
                (custom_range, index) = self.parse_ranged_repetition(c_vec, index)?;
                custom_range
            },
            '*' => (Bound::Literal(0), Bound::Literal(REPEAT_LIMIT)),
            '+' => (Bound::Literal(1), Bound::Literal(REPEAT_LIMIT)),
            '?' => (Bound::Literal(0), Bound::Literal(1)),
            _ => return Err(format!("Error: Unexpected repetition start char reached ('{}') at index {}", first_char, index)),
        };
        // removes processed characters from the char vector, returning the unprocessed remainder of the vector
        return Ok((Token::Repetition(Box::new(token), range.0, range.1), index));
    }

    fn format_input_string(&self, c_string: String) -> Vec<char> {
        return c_string.replace(" ", "").replace("  ", "").chars().collect();
    }

    pub fn get_string(token: &Token) -> String {
        return Self::recur_to_string(&token, 0);
    }

    fn recur_to_string(token: &Token, indentation: usize) -> String {
        let indent_spacing: String = " ".repeat(indentation * 2);
        return match token {
            Token::Literal(lit_string) => {
                format!("{}Literal({})", indent_spacing, lit_string)
            },
            Token::Choice(choices) => {
                let choice_strings: Vec<String> = choices.iter().map(| choice| ExpressionParser::recur_to_string(choice, indentation + 1)).collect();
                format!("{}Choice:\n{}", indent_spacing, choice_strings.join("\n"))
            },
            Token::Repetition(inner_token, lower_bound, upper_bound) => {
                let token_string: String = ExpressionParser::recur_to_string(inner_token.as_ref(), indentation + 1);
                format!("{}Repeat ({} to {}) times:\n{}", indent_spacing, Bound::get_string(lower_bound), Bound::get_string(upper_bound), token_string)
            },
            Token::Sequence(series) => {
                let sequence_strings: Vec<String> = series.iter().map(| section | ExpressionParser::recur_to_string(section, indentation + 1)).collect();
                format!("{}Sequence:\n{}", indent_spacing, sequence_strings.join("\n"))
            },
        }
    }
}

/// 
struct Expression { 
    c_token: ContextToken,
    min_length: usize,
}

impl Expression {
    pub fn from_token(&self, expression_token: ContextToken) -> Result<Expression, String> {
        let min_gen_length: usize = self.calculate_min_length(&expression_token.token);
        return Ok(Expression { c_token: expression_token, min_length: min_gen_length });
    }
    pub fn from_string(&self, expression_string: String) -> Result<Expression, String> {
        let expression_token: ContextToken = ExpressionParser::produce_token(expression_string)?;
        let min_gen_length: usize = self.calculate_min_length(&expression_token.token);
        return Ok(Expression { c_token: expression_token, min_length: min_gen_length });
    }

    pub fn gen_to_length(&self, length: usize) -> Vec<String> {
        const SAMPLES: usize = 100;
        let mut generated_strings: Vec<String> = Vec::new();

        for _ in 0..SAMPLES {
            let new_string: Result<String, String> = self.recur_to_length(&self.c_token.token, length);
            match new_string {
                Ok(string) => generated_strings.push(string),
                Err(_) => {},
            }
        }
        return generated_strings;
    }

    pub fn recur_to_length(&self, expression_token: &Token, target_length: usize) -> Result<String, String> {
        let max_length: usize = self.calculate_max_length(expression_token);
        let min_length: usize = self.calculate_min_length(expression_token);

        // if the max length reachable is below the target, or the min length is above, fail and exit
        if max_length < target_length || min_length > target_length {
            return Err("Impossible to reach target from current state".to_string());
        }
        // if target length has elapsed, exit out successfully
        if target_length == 0 {
            return Ok("".to_string());
        }

        match expression_token {
            Token::Literal(literal) => {
                return Ok(literal.to_string());
            }
            Token::Repetition(token, lower_bound, upper_bound) => {
                let context: &HashMap<String, usize> = &self.c_token.context;

                let inner_token: &Token = token.as_ref();
                let lower: usize = Bound::calculate_bound(lower_bound, context);
                let upper: usize = Bound::calculate_bound(upper_bound, context);
                let inner_min: usize = Expression::calculate_min_length(&self, inner_token);
                let inner_max: usize = Expression::calculate_max_length(&self, inner_token);

                let mut bound_targets: Vec<Vec<usize>> = Vec::new();
                let mut min_vec: Vec<usize> = vec![inner_min; lower];
                let mut max_vec: Vec<usize> = vec![inner_max; lower];
                // this may cause issues with length 0 subtokens - these should be optimised out by setting this lower bound to 0 and removing the length 0 subtoken
                // generates lists of possible partitions
                for _ in lower..upper {
                    let targets = Expression::produce_partitions(&min_vec, &max_vec, target_length);
                    if targets.is_ok() {
                        bound_targets.push(targets.unwrap());
                    }
                    else {
                        break;
                    }
                    min_vec.push(inner_min);
                    max_vec.push(inner_max);
                }

                // uses built lists of possible partitions to attempt to produce strings
                for i in 0..bound_targets.len() {
                    let next_string: Result<Vec<String>, String> = bound_targets[i].iter().map(| target | self.recur_to_length(inner_token, *target)).collect();
                    if next_string.is_ok() {
                        return Ok(next_string.unwrap().join(""));
                    }
                }
                return Err("Could not find valid string configuration for repetition".to_string());
            }
            Token::Choice(tokens) => {
                let acceptable_tokens: Vec<&Token> = tokens.iter()
                    .filter(| token | self.calculate_max_length(token) >= target_length && self.calculate_min_length(token) <= target_length)
                    .collect();

                return match acceptable_tokens.choose(&mut rng()) {
                    Some(token) => self.recur_to_length(*token, target_length),
                    None => Err("Error: Failed to find valid choice for choice token in string generation".to_string()),
                }
            }
            Token::Sequence(tokens) => {
                let component_min_lengths: Vec<usize> = tokens.iter().map(| token | self.calculate_min_length(token)).collect();
                let component_max_lengths: Vec<usize> = tokens.iter().map(| token | self.calculate_max_length(token)).collect();

                let partition_targets: Vec<usize> = Expression::produce_partitions( &component_min_lengths, &component_max_lengths, target_length)?;
                return Ok(tokens.iter().zip(partition_targets.iter())
                    .map(| (token, length) | self.recur_to_length(token, *length))
                    .collect::<Result<Vec<String>, String>>()?
                    .join(""));
            }
        }
    }

    // calculates the minimum bound of a full `Token` object
    fn calculate_min_bound(&self, token: &Token) -> Bound {
        return match token {
            Token::Literal(lit) => Bound::Literal(lit.len()),
            // repetitions are calculated by multiplying the min bound of a token by its min length, resulting in the min total
            Token::Repetition(inner_token, lower, _) => Bound::Calculation(Box::new(Expression::calculate_min_bound(&self, inner_token.as_ref())), Operation::Multiply, Box::new(lower.clone())),
            // choices are calculated through finding the min bound for the shortest possible choice token
            Token::Choice(choices) => Expression::calculate_min_bound(&self, choices.iter()
                .min_by_key(| choice | Expression::calculate_min_length(&self, *choice))
                .unwrap_or(&Token::Literal("".to_string()))),
            // sequences are calculated through producing the sum of the min bound of all sequence tokens
            Token::Sequence(sequence) => match sequence.len() {
                0 => Bound::Literal(0),
                1 => self.calculate_min_bound(sequence.first().unwrap()),
                _ => {
                    let mut result: Bound = Bound::Literal(0);
                    for i in 0..sequence.len() {
                        result = Bound::Calculation(Box::new(result), Operation::Add, Box::new(self.calculate_min_bound(sequence.get(i).unwrap())));
                    }
                    result
                }
            }
        }
    }

    // operates the same as min bound, but with some functions flipped to instead calculate the max bound
    fn calculate_max_bound(&self, token: &Token) -> Bound {
        return match token {
            Token::Literal(lit) => Bound::Literal(lit.len()),
            Token::Repetition(inner_token, _, upper) => Bound::Calculation(Box::new(Expression::calculate_max_bound(&self, inner_token.as_ref())), Operation::Multiply, Box::new(upper.clone())),
            Token::Choice(choices) => Expression::calculate_max_bound(&self, choices.iter()
                .max_by_key(| choice | Expression::calculate_max_length(&self, *choice))
                .unwrap_or(&Token::Literal("".to_string()))),
            Token::Sequence(sequence) => match sequence.len() {
                0 => Bound::Literal(0),
                1 => self.calculate_max_bound(sequence.first().unwrap()),
                _ => {
                    let mut result: Bound = Bound::Literal(0);
                    for i in 0..sequence.len() {
                        result = Bound::Calculation(Box::new(result), Operation::Add, Box::new(self.calculate_max_bound(sequence.get(i).unwrap())));
                    }
                    result
                }
            }
        }        
    }

    // biased simple partition producer
    fn produce_partitions(lower: &[usize], upper: &[usize], target_length: usize) -> Result<Vec<usize>, String> {
        let min_lower: usize = lower.iter().sum();
        let max_upper: usize = upper.iter().sum();
        let partitions: usize = lower.len();
        let mut lengths: Vec<usize> = Vec::with_capacity(partitions);

        if target_length < min_lower || target_length > max_upper {
            return Err("Target lengths are invalid for partitioning".to_string());
        }

        let mut remaining_allocation: usize = target_length - min_lower;
        // implements random bounded composition to find valid random allocations of size to each partition
        for i in 0..partitions {
            let lower_partition: usize = lower[i];
            let upper_partition: usize = upper[i];
            let max_allocation: usize = upper_partition - lower_partition;
            let allocation: usize;

            // if this is the final allocation, allocate the remainder of length to this partition
            if i == partitions - 1 {
                allocation = remaining_allocation;
            }
            // otherwise, allocate a random amount of length between 0 and the remaining allocation/max allocation to it
            else {
                allocation = rand::rng().random_range(0..=remaining_allocation.min(max_allocation));
            }
            lengths.push(lower_partition + allocation);
            remaining_allocation -= allocation;
        }

        return Ok(lengths);
    }

    fn calculate_min_length(&self, analysed_token: &Token) -> usize {
        return match analysed_token {
            Token::Literal(literal) => literal.len(),
            Token::Repetition(repeated_token, lower_bound, _) => self.calculate_min_length(repeated_token.as_ref()) * Bound::calculate_bound(lower_bound, &self.c_token.context),
            Token::Choice(token_vec) => token_vec.iter().map(| token | self.calculate_min_length(token)).min().unwrap_or(0),
            Token::Sequence(token_vec) => token_vec.iter().map(| token | self.calculate_min_length(token)).sum(),
        };
    }

    fn calculate_max_length(&self, analysed_token: &Token) -> usize {
        return match analysed_token {
            Token::Literal(literal) => literal.len(),
            Token::Repetition(repeated_token, _, upper_bound) => self.calculate_max_length(repeated_token.as_ref()) * Bound::calculate_bound(upper_bound, &self.c_token.context),
            Token::Choice(token_vec) => token_vec.iter().map(| token | self.calculate_max_length(token)).max().unwrap_or(0),
            Token::Sequence(token_vec) => token_vec.iter().map(| token | self.calculate_min_length(token)).sum(),
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

fn main() {
    let output_token: Result<ContextToken, String> = ExpressionParser::produce_token("(ab|cd|(e{x*2,}|a{,4})|f){,12}a".to_string());
    match output_token {
        Ok(tk) => {
            println!("{}", ExpressionParser::get_string(&tk.token));
        }
        Err(er) => {
            println!("{}", er);
        }
    }
}