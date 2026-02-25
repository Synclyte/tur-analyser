use std::{collections::{HashMap, HashSet}, env::var, ops::Range, usize, vec};
use rand::{Rng, rng, rngs::ThreadRng, seq::{IndexedRandom, SliceRandom}};

const REPEAT_LIMIT: i32 = 128;

// token struct for representing regular expressions
// covers all basic regex operations 
// essentially an AST for expanded regex

#[derive(PartialEq, Clone, Eq, Hash)]
enum Token {
    Literal(String),
    Repetition(Box<Token>, Bound, Bound),
    Choice(Vec<Token>),
    Sequence(Vec<Token>),
}

#[derive(Clone)]
enum CalcToken {
    Literal((i32, i8)),
    Variable((String, i8)),
    Operation((Operation, i8))
}

enum CalcTokenType {
    Literal,
    Variable,
    Operation,
    ClosingBracket,
    None
}

// context binding for a token AST
struct ContextToken {
    token: Token,
    context: HashMap<String, i32>,
}

// mathematical operations - used for calculating discrete repetition `Bound` values
#[derive(Clone, PartialEq, Eq, Hash)]
enum Operation {
    Add,
    Subtract,
    Multiply,
}

impl Operation {
    fn get_priority(op: &Operation) -> usize {
        return match op {
            Self::Add => 1,
            Self::Subtract => 1,
            Self::Multiply => 2
        }
    }

    fn char_to_operation(chr: char) -> Option<Operation> {
        return match chr {
            '+' => Some(Operation::Add),
            '-' => Some(Operation::Subtract),
            '*' => Some(Operation::Multiply),
            _ => None
        }
    }
}

// calculation tree - provides a means to calculate the value of a given mathematical expression
#[derive(PartialEq, Eq, Hash)]
enum Bound {
    Literal(i32),
    Variable(String),
    Calculation(Box<Bound>, Operation, Box<Bound>)
}

impl Clone for Bound {
    // deep copy of a bound
    fn clone(&self) -> Self {
        return match &self {
            Self::Literal(size) => Self::Literal(size.clone()),
            Self::Variable(var) => Self::Variable(var.clone()),
            Self::Calculation(bound_1, operator, bound_2) => Self::Calculation(bound_1.clone(), operator.clone(), bound_2.clone())
        }
    }
}

impl Bound {
    fn has_variable(&self, target_name: &str) -> bool {
        match self {
            Bound::Variable(var_name) => var_name == target_name,
            Bound::Calculation(left, _, right) => left.has_variable(target_name) | right.has_variable(target_name),
            _ => false,
        }
    }

    fn calculate_bound(&self, context: &HashMap<String, i32>) -> Option<i32> {
        let result = self.calculate_bound_components(context).min(REPEAT_LIMIT);
        return Some(result);
    }

    // recursively calculates a bound using dfs
    fn calculate_bound_components(&self, context: &HashMap<String, i32>) -> i32 {
        return match self {
            Bound::Literal(value) => *value,
            Bound::Variable(value) => *context.get(value).unwrap_or(&0i32),
            Bound::Calculation(first, operator, last) => {
                let first_result = Bound::calculate_bound_components(first.as_ref(), context);
                let last_result = Bound::calculate_bound_components(last.as_ref(), context);
                match operator {
                    Operation::Add => first_result + last_result,
                    Operation::Multiply => first_result * last_result,
                    Operation::Subtract => first_result - last_result,
                }
            }
        };
    }

    fn get_string(&self) -> String {
        return match self {
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

#[derive(Clone)]
struct Constraint {
    min: Bound,
    max: Bound
}

struct DependencyGraph {
    order: Vec<String>,
    constraints: Vec<Constraint>
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
    context: HashMap<String, i32>,
}

impl ExpressionParser {
    fn new() -> Self {
        return ExpressionParser { context: HashMap::new() };
    }

    /// takes a string as input, and returns either the resulting tokenised regular expression, or a string-based error. main function of ExpressionParser
    pub fn produce_token(expression: &str) -> Result<ContextToken, String> {
        let mut parser: ExpressionParser = ExpressionParser::new();
        let mut cleaned_vector_expression: Vec<char> = expression.chars().filter(|c| !c.is_whitespace()).collect();
        let output_token: Token = Self::parse_chars(&mut parser, &mut cleaned_vector_expression, '\0', 0)?.0;

        return Ok(ContextToken{token: output_token, context: parser.context});
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

        if token_vec.len() == 1 {
            return Ok((token_vec.pop().unwrap(), index));
        }

        return Ok((Token::Sequence(token_vec), index));
    }

    /// * produces a `Range<usize>` from a discrete repetition (bounded by {}, like {4,12})
    /// * returns this range alongside the number of characters used in the repetition
    fn parse_ranged_repetition(&mut self, c_vec: &Vec<char>, index: usize) -> Result<((Bound, Bound), usize), String> {
        let end_index: usize;
        // iterates through the expression characters, starting at the character after the entry to this function, until it finds the associated } to close this range
        // and then returns the index associated with that character
        let retrieved_end_index: Option<usize> = c_vec.iter()
            .enumerate()
            .skip(index + 1)
            .find(| &(_, &c) | c == '}')
            .map(|(i, _)| i);

        // if the index is not found, error out as this is an invalid range
        match retrieved_end_index {
            Some(found_index) => end_index = found_index,
            _ => return Err(format!("Failed to find end of repetition starting at index {}", index)),
        }
        // otherwise, collect all of the characters in the range and process them
        let extracted_range: String = c_vec[index..end_index].iter().collect();

        if let Some((first, last)) = extracted_range.split_once(',') {
            let lower_bound: Bound = self.get_bound_from_string(&first.to_string(), 0)?;
            let upper_bound: Bound = self.get_bound_from_string(&last.to_string(), REPEAT_LIMIT)?;

            return Ok(((lower_bound, upper_bound), end_index));
        }
        else {
            let single_bound: Bound = self.get_bound_from_string(&extracted_range, 0)?;

            return Ok(((single_bound.clone(), single_bound), end_index));
        }
    }

    fn get_bound_from_string(&mut self, string: &String, default_size: i32) -> Result<Bound, String> {
        if string.is_empty() {
            return Ok(Bound::Literal(default_size));
        }

        self.get_token_tree(&self.tokenise_arithmetic_expression(string)?)
    }

    // performs a scan of the expression and converts it into a Vec of representative tokens
    fn tokenise_arithmetic_expression(&self, expression: &String) -> Result<Vec<CalcToken>, String> {
        if expression.is_empty() {
            return Err("Expression has length 0".to_string());
        }

        let mut token_vec: Vec<CalcToken> = Vec::new();
        let mut string_stream = expression.chars().peekable();
        let mut bracket_priority: i8 = 0;
        let mut previous_type: CalcTokenType = CalcTokenType::None;

        while let Some(&chr) = string_stream.peek() {
            match chr {
                '(' => {
                    self.do_implicit_mult(&previous_type, &mut token_vec, &bracket_priority);
                    bracket_priority += 1;
                    string_stream.next();
                    // content within opening brackets should be handled as the start of a new expression
                    previous_type = CalcTokenType::None;
                },
                ')' => {
                    if bracket_priority == 0 { return Err("Invalid bracketing configuration - opening bracket required for closing bracket".into()) }
                    bracket_priority -= 1;
                    string_stream.next();
                    previous_type = CalcTokenType::ClosingBracket;
                },
                '+' | '-' | '*' => {
                    let op = Operation::char_to_operation(chr).unwrap();
                    if matches!(previous_type, CalcTokenType::Operation) || token_vec.is_empty() { return Err("Invalid operator positioning in range - operators must have two associated values".into()); }
                    token_vec.push(CalcToken::Operation((op, bracket_priority)));
                    string_stream.next();
                    previous_type = CalcTokenType::Operation;
                },
                '0'..='9' => {
                    self.do_implicit_mult(&previous_type, &mut token_vec, &bracket_priority);
                    let num = self.get_full_matching_token(&mut string_stream, |c| matches!(c, '0'..='9'));
                    token_vec.push(CalcToken::Literal((num.parse::<i32>().unwrap(), bracket_priority)));
                    previous_type = CalcTokenType::Literal;
                },
                'a'..='z' | 'A'..='Z' => {
                    self.do_implicit_mult(&previous_type, &mut token_vec, &bracket_priority);
                    let var = self.get_full_matching_token(&mut string_stream, |c| matches!(c, 'a'..='z' | 'A'..='Z'));
                    token_vec.push(CalcToken::Variable((var, bracket_priority)));
                    previous_type = CalcTokenType::Variable;
                }
                _ => return Err(format!("Invalid character found in range: '{}'", chr))
            }
        }

        Ok(token_vec)
    }

    // expressions like 2x or 2(5) imply multiplication. this adds a multiply operation in these cases
    fn do_implicit_mult(&self, previous_type: &CalcTokenType, token_vec: &mut Vec<CalcToken>, bracket_priority: &i8) {
        if matches!(previous_type, CalcTokenType::Literal | CalcTokenType::Variable | CalcTokenType::ClosingBracket) {
            token_vec.push(CalcToken::Operation((Operation::Multiply, *bracket_priority)));
        }
    }

    // scans the string stream to extract the full token, with matches determined by the take_fn, and returns the full match as a string
    fn get_full_matching_token(&self, string_stream: &mut std::iter::Peekable<std::str::Chars<'_>>, take_fn: impl Fn(char) -> bool) -> String {
        let mut collected_string = String::new(); 
        while let Some(&chr) = string_stream.peek() {
            if take_fn(chr) { collected_string.push(string_stream.next().unwrap()) }
            else { break }
        }
        collected_string
    }

    fn get_token_tree(&mut self, tokens: &[CalcToken]) -> Result<Bound, String> {
        if tokens.is_empty() {
            return Err("Malformed token array given - array has length 0".into());
        }

        let (index, token) = tokens.iter()
            .enumerate()
            .rev()
            .min_by_key(| (_, a) | self.get_priority(a))
            .unwrap();

        return match token {
            CalcToken::Operation((op, _)) => {
                let left: Bound = self.get_token_tree(&tokens[0..index])?;
                let right: Bound = self.get_token_tree(&tokens[index + 1..])?;

                Ok(Bound::Calculation(Box::new(left), op.clone(), Box::new(right)))
            }
            CalcToken::Literal((val, _)) => Ok(Bound::Literal(*val)),
            CalcToken::Variable((var, _)) => {
                self.context.insert(var.clone(), 0);
                Ok(Bound::Variable(var.clone()))
            }
        }
    }

    fn get_priority(&self, token: &CalcToken) -> (i8, usize) {
        return match token {
            CalcToken::Operation((op, bracket_priority)) => (*bracket_priority, Operation::get_priority(op)),
            _ => (i8::MAX, usize::MAX),
        }
    }

 /*    // left to right, multiplication then add/subtract
    fn parse_arithmetic_expression(&mut self, expression: &String) -> Result<Bound, String> {
        if expression.is_empty() {
            return Err("Range expression is invalid - operators must have two arguments".into());
        }

        let mult_location: usize = expression.find("*").unwrap_or_else(|| usize::MAX);
        let add_location: usize = expression.find("+").unwrap_or_else(|| usize::MAX);
        let sub_location: usize = expression.find("-").unwrap_or_else(|| usize::MAX);

        // handles operations first
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
        let parsed_literal = expression.parse::<i32>();
        return match parsed_literal {
            Ok(literal) => Ok(Bound::Literal(literal)),
            Err(_) => Err(format!("Error: Range literal ('{}') could not be parsed", expression)),
        }
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

    */

    /// * produces a `Token::Repetition` from a generic repetition (either special chars (*, +, ?), or a discrete repetition bounded by {})
    /// * processes a given `Vec<char>` to construct this repetition, removing used characters
    fn parse_repetition(&mut self, token: Token, c_vec: &Vec<char>, mut index: usize) -> Result<(Token, usize), String> {
        let first_char: char = c_vec[index];

        // separates discrete repetitions from symbol-based repetitions
        let custom_range: (Bound, Bound);
        let range: (Bound, Bound) = match first_char {
            '{' => {
                (custom_range, index) = self.parse_ranged_repetition(c_vec, index + 1)?;
                custom_range
            },
            '*' => (Bound::Literal(0), Bound::Literal(REPEAT_LIMIT)),
            '+' => (Bound::Literal(1), Bound::Literal(REPEAT_LIMIT)),
            '?' => (Bound::Literal(0), Bound::Literal(1)),
            _ => return Err(format!("Unexpected repetition start char reached ('{}') at index {}", first_char, index)),
        };

        // removes processed characters from the char vector, returning the unprocessed remainder of the vector
        return Ok((Token::Repetition(Box::new(token), range.0, range.1), index));
    }

    fn format_input_string(&self, c_string: String) -> Vec<char> {
        return c_string.chars()
            .filter(|c| !c.is_whitespace())
            .collect();
    }

    pub fn get_string(token: &Token) -> String {
        return Self::recur_to_string(&token, 0);
    }

    // converts a given token into its string representation
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

/// designed to generate strings to a given length from a `ContextToken`, matching the inner expression
impl ContextToken {
    pub fn generate_strings_in_range(&self, search_range: Range<usize>) -> Vec<String> {
        let mut generated_strings: Vec<String> = Vec::new();
        let mut lengths_generated: HashSet<usize> = HashSet::new();

        // used to ensure that repairs to parameters are done in the correct order such that
        // they will always generate a non-contradicting set of output variables  
        let dependency_graph = self.get_dependency_graph();

        for target_length in search_range.clone() {
            let valid_vars = self.get_valid_variable_values(target_length, &dependency_graph);
            let generated_string = self.generate_string_to_length(&self.token, &valid_vars, target_length.clone());
            let valid_string = if let Some(inner_string) = generated_string {
                inner_string
            } else {
                continue;
            };

            let generated_length = valid_string.len();

            if !lengths_generated.contains(&generated_length) && search_range.contains(&generated_length) {
                generated_strings.push(valid_string);
                lengths_generated.insert(generated_length);
            }
        }
        generated_strings.sort_by_key(|s| s.len());

        return generated_strings;
    }

    // get all lengths possible by each token (get_possible_token_lengths, recursive DP-based generator)
    // reject instantly if this is not possible
    // or generate through backtracking from possible lengths. should always produce a correct string of length n
    fn generate_string_to_length(&self, token: &Token, variables: &HashMap<String, i32>, target_length: usize) -> Option<String> {
        // gets all token lengths for this input and builds the length HashSet cache
        let possible_lengths=  self.get_possible_token_lengths(token, variables, target_length);
        if !possible_lengths.contains(&target_length) {
            return None;
        }
        
        self.generate_exact_length_string(token, variables, target_length, target_length)
    }

    fn generate_exact_length_string(&self, token: &Token, variables: &HashMap<String, i32>, target_length: usize, length_limit: usize) -> Option<String> {
        let mut rng = rand::rng();

        match token {
            Token::Literal(literal) => if literal.len() == target_length {
                Some(literal.clone())
            } else {
                None
            },
            Token::Choice(tokens) => {
                let mut valid_choices: Vec<&Token> = tokens.iter()
                    .filter(| tok | self.get_possible_token_lengths(*tok, variables, length_limit).contains(&target_length))
                    .collect();

                if valid_choices.is_empty() {
                    return None;
                }

                let choice = valid_choices[rng.random_range(0..valid_choices.len())];
                self.generate_exact_length_string(choice, variables, target_length, length_limit)
            },
            Token::Repetition(repetition_token, lower_bound, upper_bound) => {
                let lower_result = lower_bound.calculate_bound(variables).unwrap_or(0).max(0) as usize;
                let upper_result = upper_bound.calculate_bound(variables).unwrap_or(0).max(0) as usize;

                // treat repetitions as a sequence
                let mut min_length = usize::MAX;
                for length in self.get_possible_token_lengths(repetition_token, variables, length_limit).iter() {
                    if length < &min_length {
                        min_length = length.clone();
                    }
                    if min_length == 0 {
                        break;
                    }
                }
                if min_length == usize::MAX {
                    return None;
                }

                // produce a vec of all "valid" repetition counts within the range provided and shuffle them so they can be randomly attempted
                let mut valid_repetitions: Vec<usize> = (lower_result..=upper_result)
                    .filter(| count | min_length * count <= target_length)
                    .collect();
                valid_repetitions.shuffle(&mut rng);

                for repetition_count in valid_repetitions {
                    // form a sequence token instead, have this be solved by the sequence solver
                    let sequence_token = Token::Sequence(vec![repetition_token.as_ref().clone(); repetition_count]);
                    if let Some(valid_string) = self.generate_exact_length_string(&sequence_token, variables, target_length, length_limit) {
                        return Some(valid_string);
                    }
                }
                None
            },
            Token::Sequence(tokens) => {
                self.generate_valid_sequence_partition(tokens, variables, 0, target_length, length_limit)
            }
        }
    }

    fn generate_valid_sequence_partition(&self, tokens: &[Token], variables: &HashMap<String, i32>, index: usize, remaining_length: usize, length_limit: usize) -> Option<String> {
        // recursive exit case
        if index == tokens.len() {
            if remaining_length == 0 {
                return Some(String::new());
            }
            return None;
        }

        // gets all possible token lengths which would work for generating a string of length n
        // and then shuffles them such that generated strings have variety
        let current_token = tokens.get(index).unwrap();
        let mut possible_token_lengths: Vec<usize> = self.get_possible_token_lengths(current_token, variables, remaining_length)
            .into_iter()
            .filter(|&l| l <= remaining_length)
            .collect();

        let mut rng = rand::rng();
        possible_token_lengths.shuffle(&mut rng);

        let next_tokens: Vec<Token> = tokens.iter().skip(index + 1).cloned().collect();
        let sequence_remainder = if !next_tokens.is_empty() {
            Some(Token::Sequence(next_tokens))
        } else {
            None
        };

        // partitioner logic - repeatedly evaluates whether making this partition would make reaching 
        // the target length impossible, and takes it if it won't
        for &possible_length in &possible_token_lengths {
            let partition_length = remaining_length - possible_length;

            // prune branches of expression which are unable to ever produce the correct length
            if let Some(remainder) = &sequence_remainder {
                let possible_remainder_lengths = self.get_possible_token_lengths(remainder, variables, length_limit);
                if !possible_remainder_lengths.contains(&partition_length) {
                    continue;
                }
            } else {
                if partition_length != 0 {
                    continue;
                }
            }

            // recursively generate the rest of this partition
            if let Some(partition_remainder) = self.generate_valid_sequence_partition(tokens, variables, index + 1, partition_length, length_limit) {
                // if the rest of the partition can be valid, generate the current part
                if let Some(generated_partition) = self.generate_exact_length_string(current_token, variables, possible_length, length_limit) {
                    return Some(generated_partition + &partition_remainder);
                }
            }
        }
        None
    }

    fn get_possible_token_lengths(&self, token: &Token, variables: &HashMap<String, i32>, length_limit: usize) -> HashSet<usize> {
        let mut lengths = HashSet::new();

        match token {
            Token::Literal(literal) => {
                lengths.insert(literal.len());
            },
            Token::Choice(tokens) => {
                for token in tokens {
                    lengths.extend(self.get_possible_token_lengths(token, variables, length_limit));
                }
            },
            Token::Repetition(repeated_token, lower, upper) => {
                let lower_result = lower.calculate_bound(variables).unwrap_or(0).max(0);
                let upper_result = upper.calculate_bound(variables).unwrap_or(0).max(0);
                let repetition_token_lengths = self.get_possible_token_lengths(repeated_token, variables, length_limit);
                let mut valid_lengths: HashSet<usize> = HashSet::new();

                // holds all of the possible lengths reachable from the current repetition iteration
                let mut current_cache = HashSet::from([0]);

                if lower_result == 0 {
                    valid_lengths.insert(0);
                }

                // iteratively builds a cache of lengths
                // this is not fast whatsoever, as it iterates through an increasing number of lengths
                // (bounded by range) every iteration for every repetition. nested repetitions can destroy performance
                for count in 1..=upper_result {
                    let mut next_cache = HashSet::new();
                    for &current_length in &current_cache {
                        for &repetition_length in &repetition_token_lengths {
                            let combined_length = current_length + repetition_length;

                            // if the length is within acceptable limit, add it to the next cache
                            if combined_length <= length_limit {
                                next_cache.insert(combined_length);
                                // and if it is above the lower bound, add it to the possible results
                                if count >= lower_result {
                                    valid_lengths.insert(combined_length);
                                }
                            }
                        }
                    }
                    current_cache = next_cache;

                    // if there are no more values to explore, then exit this loop
                    if current_cache.is_empty() {
                        break;
                    }
                }
                lengths = valid_lengths;
            },
            Token::Sequence(tokens) => {
                let mut current_cache = HashSet::from([0]);
                // could track another range here and decrement based on the minimum length in token_lengths
                // very similar, but simpler solution to repetitions
                for s_token in tokens {
                    let token_lengths = self.get_possible_token_lengths(s_token, variables, length_limit);
                    let mut next_cache: HashSet<usize> = HashSet::new();

                    for &current_length in &current_cache {
                        for &token_length in &token_lengths {
                            let combined_length = current_length + token_length;

                            if combined_length <= length_limit {
                                next_cache.insert(combined_length);
                            }
                        }
                    }
                    current_cache = next_cache;

                    if current_cache.is_empty() {
                        break;
                    }
                }
                
                lengths = current_cache;
            }
        }

        lengths
    }

    /// uses an annealing-based approach to find valid variables based on constraints
    fn get_valid_variable_values(&self, target_length: usize, dependency_graph: &DependencyGraph) -> HashMap<String, i32> {
        // gets the difference between the min/max lengths and the target value
        let calculate_difference_from_target = | variables: &HashMap<String, i32> | -> usize {
            let min_length = self.calculate_min_length(&self.token, variables);
            let max_length = self.calculate_max_length(&self.token, variables);

            return if target_length < min_length {
                min_length - target_length
            } else if target_length > max_length {
                target_length - max_length
            } else {
                0
            }
        };

        // estimation of iterations necessary - not a great heuristic and may require tweaking, but more performant than nothing
        let max_iterations = 300 + dependency_graph.order.len() * 400;

        let mut rng = rand::rng();
        let mut var_state = self.context.clone();
        Self::enforce_constraints(&mut var_state, &dependency_graph);

        let mut best_vars = var_state.clone();
        let mut best_diff = calculate_difference_from_target(&var_state);
        let init_temp = target_length as f64;

        // for each iteration, run a basic annealer:
        // mutate a random variable
        // fix broken constraints such that variables do not violate bound ranges
        // evaluate whether to accept the new variable set based on:
        //  always accept if it improves the result
        //  sometimes accept bad results based on the temperature to attempt to escape local maxima
        for i in 0..max_iterations {
            if best_diff == 0 {
                return best_vars;
            }

            let temp = init_temp * (1f64 - i as f64 / max_iterations as f64);
            let mut mutated_vars = var_state.clone();
            Self::mutate_variable(&mut mutated_vars, &dependency_graph.order, &mut rng);
            Self::enforce_constraints(&mut mutated_vars, &dependency_graph);
            let mutated_diff = calculate_difference_from_target(&mutated_vars);

            let length_change = mutated_diff as f64 - best_diff as f64;
            let acceptance_probability: f64 = if length_change < 0.0 { 1.0 }
            else { (-length_change / temp).exp() };

            if rng.random_range(0.0..=1.0) <= acceptance_probability {
                var_state = mutated_vars;

                if mutated_diff < best_diff {
                    best_vars = var_state.clone();
                    best_diff = mutated_diff;
                }
            }
        }
        return best_vars;
    }

    // applies a mutation to a random variable within a variable set, modifying one value by a random amount
    fn mutate_variable(variables: &mut HashMap<String, i32>, names: &Vec<String>, rng: &mut ThreadRng) {
        if names.is_empty() {
            return;
        }

        let target_var = names.get(rng.random_range(0..names.len())).unwrap();
        let target_value = variables.get_mut(target_var).unwrap();

        // apply a random 'mutation', modifying the variable value
        // simplified normal distribution curve with 3 steps - could also use rand_distr module instead
        *target_value = (*target_value + match rng.random_range(0..100) {
            0..60 => rng.random_range(-1..=1),
            60..85 => rng.random_range(-3..=3),
            _ => rng.random_range(-6..=6),
        }).max(0);
    }

    // simple recursive max length calculator
    fn calculate_max_length(&self, analysed_token: &Token, context: &HashMap<String, i32>) -> usize {
        return match analysed_token {
            Token::Literal(literal) => literal.len(),
            Token::Repetition(repeated_token, _, upper_bound) => self.calculate_max_length(repeated_token.as_ref(), &context) * Bound::calculate_bound(upper_bound, context).unwrap_or(0).max(0) as usize,
            Token::Choice(token_vec) => token_vec.iter().map(| token | self.calculate_max_length(token, &context)).max().unwrap_or(0),
            Token::Sequence(token_vec) => token_vec.iter().map(| token | self.calculate_max_length(token, &context)).sum(),
        }
    }

    fn calculate_min_length(&self, analysed_token: &Token, context: &HashMap<String, i32>) -> usize {
        return match analysed_token {
            Token::Literal(literal) => literal.len(),
            Token::Repetition(repeated_token, lower_bound, _) => self.calculate_min_length(repeated_token.as_ref(), &context) * Bound::calculate_bound(lower_bound, context).unwrap_or(0).max(0) as usize,
            Token::Choice(token_vec) => token_vec.iter().map(| token | self.calculate_min_length(token, &context)).min().unwrap_or(0),
            Token::Sequence(token_vec) => token_vec.iter().map(| token | self.calculate_min_length(token, &context)).sum(),
        }
    }

    fn calculate_max_bound(&self, token: &Token, context: &HashMap<String, i32>) -> Bound {
        return match token {
            Token::Literal(lit) => Bound::Literal(lit.len() as i32),
            Token::Repetition(inner_token, _, upper) => Bound::Calculation(Box::new(self.calculate_max_bound(inner_token.as_ref(), &context)), Operation::Multiply, Box::new(upper.clone())),
            Token::Choice(choices) => self.calculate_max_bound(choices.iter()
                .max_by_key(| choice | self.calculate_max_length(*choice, &context))
                .unwrap_or(&Token::Literal("".to_string())), &context),
            Token::Sequence(sequence) => match sequence.len() {
                0 => Bound::Literal(0),
                1 => self.calculate_max_bound(sequence.first().unwrap(), &context),
                _ => {
                    let mut result: Bound = Bound::Literal(0);
                    for i in 0..sequence.len() {
                        result = Bound::Calculation(Box::new(result), Operation::Add, Box::new(self.calculate_max_bound(sequence.get(i).unwrap(), &context)));
                    }
                    result
                }
            }
        }        
    }

    /// implementation of Kahn's algorithm (topological sorting) https://en.wikipedia.org/wiki/Topological_sorting#Kahn's_algorithm
    /// essentially just a rust translation of the wikipedia page pseudocode
    fn get_dependency_graph(&self) -> DependencyGraph {
        let mut adjacency_graph: HashMap<String, HashSet<String>> = HashMap::new();
        let mut constraints: Vec<Constraint> = Vec::new();
        
        Self::get_constraints(&self.token, &mut constraints, &mut adjacency_graph);
        let mut elements: HashSet<String> = HashSet::new();
        let mut vert_degrees: HashMap<String, u16> = HashMap::new();

        // preprocessing for Kahn's algorithm - building data structures
        for (key_var, proximity) in &adjacency_graph {
            elements.insert(key_var.clone());
            for var in proximity {
                elements.insert(var.clone());
                *vert_degrees.entry(var.clone()).or_insert(0) += 1;
            }
        }

        // produces set of nodes with no incoming edge to be processed. does not technically have to be a queue
        let mut element_queue: Vec<String> = elements.iter()
            .filter(|e| *vert_degrees.get(*e).unwrap_or(&0) == 0)
            .cloned()
            .collect();
        element_queue.reverse();
        let mut order: Vec<String> = Vec::new();

        // iterates through all of the nodes with no incoming edges
        // for each one, adds it to the `order` and decrements the degrees of all nodes which
        // this node shares a vertex with, as that vertex is no longer valid
        // Kahn's algorithm pseudocode - https://en.wikipedia.org/wiki/Topological_sorting#Kahn's_algorithm
        while let Some(e) = element_queue.pop() {
            order.push(e.clone());
            if let Some(adj) = &adjacency_graph.get(&e) {
                for adj_e in *adj {
                    let mut degree = *vert_degrees.get(adj_e).unwrap();
                    degree -= 1;
                    if degree == 0 {
                        element_queue.insert(0, adj_e.clone());
                    }
                }
            }
        }

        // some elements will not work with this, as this algorithm assumes a DAG is input
        // therefore, for cycles, an arbitrary order must be given
        for cyclic_element in elements {
            if !order.contains(&cyclic_element) {
                order.push(cyclic_element);
            }
        }
        
        return DependencyGraph { order: order, constraints }
    }

    /// fixes constraints post-mutation such that all are fulfilled
    fn enforce_constraints(variables: &mut HashMap<String, i32>, dependency_graph: &DependencyGraph) {
        let search_limit: i32 = 100;

        // tracks constraints which have already been modified such that they are not modified again
        let mut processed_vars: HashSet<String> = HashSet::new();

        // for each variable, ensures that all constraints are fulfilled
        for var in &dependency_graph.order {
            processed_vars.insert(var.clone());
            // for the current variable, get all constraints which involve this variable
            let var_value: i32 = *variables.get(var).unwrap_or(&0);
            let var_constraints: Vec<&Constraint> = dependency_graph.constraints
                .iter()
                .filter(|c| {
                    let min_vars = Self::get_variables(&c.min);
                    let max_vars = Self::get_variables(&c.max);

                    // constraint should only be added if the current var is involved in it and the 
                    // constraint has not already been solved 
                    let var_is_relevant = min_vars.contains(var) || max_vars.contains(var);
                    let all_vars_processed = min_vars.iter().all(| v | processed_vars.contains(v)) &&
                        max_vars.iter().all(| v | processed_vars.contains(v));

                    return var_is_relevant && all_vars_processed;
                }).collect();

            // skip if no constraints rely on this value - this var is unused
            if var_constraints.len() == 0 {
                continue;
            }

            // otherwise, iterate through values and find the closest value which is valid
            let mut closest_val = var_value;
            let mut valid_found = false;

            let satisfies_constraints = | variables: &mut HashMap<String, i32>, var_constraints: &Vec<&Constraint> | -> bool {
                let mut is_valid = true;
                for constraint in var_constraints {
                    let lower_bound = constraint.min.calculate_bound(variables).unwrap_or(0);
                    let upper_bound = constraint.max.calculate_bound(variables).unwrap_or(0);

                    if lower_bound > upper_bound {
                        is_valid = false;
                        break;
                    }
                }
                return is_valid;            
            };
            
            // checks based on distance from current value - always looks for closest
            for possible_val in 0..=search_limit { 
                let test_values = [var_value + possible_val, var_value - possible_val];
                for value in test_values {
                    // modifies the variable hashmap, then tests the variables. when the first valid instance
                    // is found, exits out and accepts the found variable set
                    variables.insert(var.clone(), value);
                    if &value >= &0 && satisfies_constraints(variables, &var_constraints) {
                        closest_val = value;
                        valid_found = true;
                        break;
                    }
                }
                if valid_found {
                    break;
                }
            }

            // if no valid variable configuration is found, uses the original value
            variables.insert(var.clone(), closest_val);
        }
    }

    /// recursively gathers constraints relating to the variables within ranges in `Token`s
    fn get_constraints(token: &Token, constraints: &mut Vec<Constraint>, adjacency_graph: &mut HashMap<String, HashSet<String>>) {
        return match token {
            Token::Sequence(t) | Token::Choice(t) => {
                for token in t { Self::get_constraints(token, constraints, adjacency_graph); }
            },
            Token::Repetition(inner_token, min, max) => {
                constraints.push(Constraint { min: min.clone(), max: max.clone() });
                // adds the variables of the max range to the adjacency graph of the min range
                for min_var in Self::get_variables(min) {
                    for max_var in Self::get_variables(max) {
                        adjacency_graph.entry(min_var.clone()).or_default().insert(max_var.clone());
                    }
                }
                Self::get_constraints(inner_token, constraints, adjacency_graph);
            },
            _ => {}
        }
    }

    // recursively extracts variables from a provided bound and adds them to a hashset for use in
    // the adjacency graph
    fn get_variables(bound: &Bound) -> HashSet<String> {
        let mut contained_vars: HashSet<String> = HashSet::new();
        match bound {
            Bound::Variable(var) => {
                contained_vars.insert(var.clone());
            },
            Bound::Calculation(left, _, right) => {
                contained_vars.extend(Self::get_variables(left));
                contained_vars.extend(Self::get_variables(right));
            },
            _ => {}
        }
        return contained_vars;
    }
}

fn main() {
    let output_token: Result<ContextToken, String> = ExpressionParser::produce_token("(ab|cd|(e{x*2,}|a{,4})|f){,12}a");
    match output_token {
        Ok(tk) => println!("{}", ExpressionParser::get_string(&tk.token)),
        Err(er) => println!("{}", er),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn literal(lit: &str) -> Token {
        Token::Literal(lit.into())
    }

    fn sequence(tokens: &[Token]) -> Token {
        Token::Sequence(tokens.to_vec())
    }

    fn choice(tokens: &[Token]) -> Token {
        Token::Choice(tokens.to_vec())
    }

    fn repetition(inner_token: Token, lower_bound: Bound, upper_bound: Bound) -> Token {
        Token::Repetition(Box::new(inner_token), lower_bound, upper_bound)
    }

    mod parser_tests {
        use super::*;

        fn to_char_vec(chars: &str) -> Vec<char> {
            chars.chars().collect::<Vec<char>>()
        }

        // bound tests
        mod bounds {
            use super::*;

            #[test]
            fn multiply_literals_in_bound() {
                let ctx: HashMap<String, i32> = HashMap::new();
                let test_calculation: Bound = Bound::Calculation(Box::new(Bound::Literal(5)), Operation::Multiply, Box::new(Bound::Literal(10)));
                let result = Bound::calculate_bound(&test_calculation, &ctx);
                assert_eq!(result.unwrap(), 50);
            }

            #[test]
            fn add_literals_in_bound() {
                let ctx: HashMap<String, i32> = HashMap::new();
                let test_calculation: Bound = Bound::Calculation(Box::new(Bound::Literal(5)), Operation::Add, Box::new(Bound::Literal(10)));
                let result = Bound::calculate_bound(&test_calculation, &ctx);
                assert_eq!(result.unwrap(), 15);
            }

            #[test]
            fn subtract_literals_in_bound() {
                let ctx: HashMap<String, i32> = HashMap::new();
                let test_calculation: Bound = Bound::Calculation(Box::new(Bound::Literal(10)), Operation::Subtract, Box::new(Bound::Literal(5)));
                let result = Bound::calculate_bound(&test_calculation, &ctx);
                assert_eq!(result.unwrap(), 5);
            }

            #[test]
            fn calculate_multiple_literal_operations_in_bound() {
                let ctx: HashMap<String, i32> = HashMap::new();
                let sub_calculation: Bound = Bound::Calculation(Box::new(Bound::Literal(10)), Operation::Subtract, Box::new(Bound::Literal(8)));
                let add_calculation: Bound = Bound::Calculation(Box::new(Bound::Literal(3)), Operation::Add, Box::new(sub_calculation));
                let mult_calculation: Bound = Bound::Calculation(Box::new(Bound::Literal(5)), Operation::Multiply, Box::new(add_calculation));
                let result = Bound::calculate_bound(&mult_calculation, &ctx);
                assert_eq!(result.unwrap(), 25);
            }

            #[test]
            fn calculate_simple_literal_bound_from_string() {
                let mut parser = ExpressionParser::new();
                let bound_str: String = "5".into();
                let bound = parser.get_bound_from_string(&bound_str, 0).unwrap();
                let result = Bound::calculate_bound(&bound, &parser.context);
                assert_eq!(result.unwrap(), 5);
            }

            #[test]
            fn calculate_complex_literal_bound_from_string() {
                let mut parser = ExpressionParser::new();
                let bound_str = "5*4-10*2".into();
                let bound = parser.get_bound_from_string(&bound_str, 0).unwrap();
                let result = Bound::calculate_bound(&bound, &parser.context);
                assert_eq!(result.unwrap(), 0);
            }

            #[test]
            fn calculate_simple_var_bound_from_string() {
                let mut parser = ExpressionParser::new();
                let bound_str = "a".into();
                let bound = parser.get_bound_from_string(&bound_str, 0).unwrap();
                parser.context.insert("a".into(), 4);
                let result = Bound::calculate_bound(&bound, &parser.context);
                assert_eq!(result.unwrap(), 4);            
            }

            #[test]
            fn calculate_complex_var_bound_from_string() {
                let mut parser = ExpressionParser::new();
                let bound_str = "a*b-c+a".into();
                let bound = parser.get_bound_from_string(&bound_str, 0).unwrap();
                parser.context.insert("a".into(), 5);
                parser.context.insert("b".into(), 4);
                parser.context.insert("c".into(), 10);
                let result = Bound::calculate_bound(&bound, &parser.context);
                assert_eq!(result.unwrap(), 15);
            }

            #[test]
            fn calculate_simple_mixed_bound_from_string() {
                let mut parser = ExpressionParser::new();
                let bound_str = "5*a-10".into();
                let bound = parser.get_bound_from_string(&bound_str, 0).unwrap();
                parser.context.insert("a".into(), 4);
                let result = Bound::calculate_bound(&bound, &parser.context);
                assert_eq!(result.unwrap(), 10);
            }

            #[test]
            fn calculate_complex_mixed_bracketed_bound_from_string() {
                let mut parser = ExpressionParser::new();
                let bound_str = "(2-1)*3+(3*(7-4)-1)-2".into();
                let bound = parser.get_bound_from_string(&bound_str, 0).unwrap();
                let result = Bound::calculate_bound(&bound, &parser.context);
                assert_eq!(result.unwrap(), 9);
            }

            #[test]
            fn calculate_simple_literal_implicit_mult_from_string() {
                let mut parser = ExpressionParser::new();
                let bound_str = "(2)(3)(4)".into();
                let bound = parser.get_bound_from_string(&bound_str, 0).unwrap();
                let result = Bound::calculate_bound(&bound, &parser.context);
                assert_eq!(result.unwrap(), 24);      
            }

            #[test]
            fn calculate_complex_mixed_implicit_bracketed_bound_from_string() {
                let mut parser = ExpressionParser::new();
                let bound_str = "5(6)+2x+2(3)(4)-3(2(2))".into();
                let bound = parser.get_bound_from_string(&bound_str, 0).unwrap();
                parser.context.insert("x".into(), 5);
                let result = Bound::calculate_bound(&bound, &parser.context);
                assert_eq!(result.unwrap(), 52);
            }

            #[test]
            fn calculate_simple_bracketed_mult_from_string() {
                let mut parser = ExpressionParser::new();
                let bound_str = "(2)(3)*(4)".into();
                let bound = parser.get_bound_from_string(&bound_str, 0).unwrap();
                let result = Bound::calculate_bound(&bound, &parser.context);
                assert_eq!(result.unwrap(), 24); 
            }

            #[test]
            fn error_when_invalid_operations() {
                let mut parser = ExpressionParser::new();
                let bound_str = "5+(+5)".into();
                assert!(parser.get_bound_from_string(&bound_str, 0).is_err());
            }

            #[test]
            fn error_when_invalid_bracketing() {
                let mut parser = ExpressionParser::new();
                let bound_str = ")5+4(".into();
                assert!(parser.get_bound_from_string(&bound_str, 0).is_err());
            }
        }

        mod repetition_parser {
            use super::*;

            fn compare_token_bounds_to_expectations(result: Token, a_result: i32, b_result: i32) {
                match result {
                    Token::Repetition(_, Bound::Literal(a), Bound::Literal(b)) => {
                        assert!(a == a_result && b == b_result);
                    }
                    _ => panic!()          
                }
            }

            #[test]
            fn kleene_star_produces_correct_range() {
                let mut parser = ExpressionParser::new();
                let sequence_token: Token = sequence(&[literal("a")]);
                let captured_chars: &Vec<char> = &vec!['*'];
                let result_token = parser.parse_repetition(sequence_token, captured_chars, 0).unwrap().0;
                compare_token_bounds_to_expectations(result_token, 0, REPEAT_LIMIT);
            }

            #[test]
            fn kleene_plus_produces_correct_range() {
                let mut parser = ExpressionParser::new();
                let sequence_token: Token = sequence(&[literal("a")]);
                let captured_chars: &Vec<char> = &vec!['+'];
                let result_token = parser.parse_repetition(sequence_token, captured_chars, 0).unwrap().0;
                compare_token_bounds_to_expectations(result_token, 1, REPEAT_LIMIT);
            }

            #[test]
            fn question_mark_produces_correct_range() {
                let mut parser = ExpressionParser::new();
                let sequence_token: Token = sequence(&[literal("a")]);
                let captured_chars: &Vec<char> = &vec!['?'];
                let result_token = parser.parse_repetition(sequence_token, captured_chars, 0).unwrap().0;
                compare_token_bounds_to_expectations(result_token, 0, 1);
            }

            #[test]
            fn simple_discrete_range_correctly_produced() {
                let mut parser = ExpressionParser::new();
                let sequence_token: Token = sequence(&[literal("a")]);
                let captured_chars: &Vec<char> = &to_char_vec("{1,3}");
                let result_token = parser.parse_repetition(sequence_token, captured_chars, 0).unwrap().0;
                compare_token_bounds_to_expectations(result_token, 1, 3);            
            }

            #[test]
            fn lower_inferred_discrete_range_correctly_produced() {
                let mut parser = ExpressionParser::new();
                let sequence_token: Token = sequence(&[literal("a")]);
                let captured_chars: &Vec<char> = &to_char_vec("{,3}");
                let result_token = parser.parse_repetition(sequence_token, captured_chars, 0).unwrap().0;
                compare_token_bounds_to_expectations(result_token, 0, 3);           
            }

            #[test]
            fn upper_inferred_discrete_range_correctly_produced() {
                let mut parser = ExpressionParser::new();
                let sequence_token: Token = sequence(&[literal("a")]);
                let captured_chars: &Vec<char> = &to_char_vec("{1,}");
                let result_token = parser.parse_repetition(sequence_token, captured_chars, 0).unwrap().0;
                compare_token_bounds_to_expectations(result_token, 1, REPEAT_LIMIT);            
            }

            #[test]
            fn double_inferred_discrete_range_correctly_produced() {
                let mut parser = ExpressionParser::new();
                let sequence_token: Token = sequence(&[literal("a")]);
                let captured_chars: &Vec<char> = &to_char_vec("{,}");
                let result_token = parser.parse_repetition(sequence_token, captured_chars, 0).unwrap().0;
                compare_token_bounds_to_expectations(result_token, 0, REPEAT_LIMIT);            
            }

            #[test]
            fn boundary_discrete_range_correctly_produced() {
                let mut parser = ExpressionParser::new();
                let sequence_token: Token = sequence(&[literal("a")]);
                let boundary_range_string: String = format!("{{{},{}}}", REPEAT_LIMIT, REPEAT_LIMIT);
                let captured_chars: &Vec<char> = &boundary_range_string.chars().collect();
                let result_token = parser.parse_repetition(sequence_token, captured_chars, 0).unwrap().0;
                compare_token_bounds_to_expectations(result_token, REPEAT_LIMIT, REPEAT_LIMIT);             
            }

            #[test]
            fn calculate_mixed_implicit_ranges() {
                let mut parser = ExpressionParser::new();
                let sequence_token: Token = sequence(&[literal("a")]);
                let captured_chars: &Vec<char> = &to_char_vec("{(2+a)b,(b)(((b)}");
                let result_token = parser.parse_repetition(sequence_token, captured_chars, 0).unwrap().0;

                parser.context.insert("a".into(), 2);
                parser.context.insert("b".into(), 5);

                match result_token {
                    Token::Repetition(_, bound_a, bound_b) => {
                        let a = Bound::calculate_bound(&bound_a, &parser.context).unwrap();
                        let b = Bound::calculate_bound(&bound_b, &parser.context).unwrap();
                        assert_eq!(a, 20);
                        assert_eq!(b, 25);
                    }
                    _ => panic!()
                }
            }

            #[test]
            fn calculate_complex_literal_ranges_with_leading_zeros() {
                let mut parser = ExpressionParser::new();
                let sequence_token: Token = sequence(&[literal("a")]);
                let captured_chars: &Vec<char> = &to_char_vec("{01+2+3+4,(1+2)-3*02+4*(4}");
                let result_token = parser.parse_repetition(sequence_token, captured_chars, 0).unwrap().0;

                parser.context.insert("a".into(), 2);
                parser.context.insert("b".into(), 5);

                match result_token {
                    Token::Repetition(_, bound_a, bound_b) => {
                        let a = Bound::calculate_bound(&bound_a, &parser.context).unwrap();
                        let b = Bound::calculate_bound(&bound_b, &parser.context).unwrap();
                        assert_eq!(a, 10);
                        assert_eq!(b, 13);
                    }
                    _ => panic!()
                }
            }

            #[test]
            fn error_on_invalid_range_character() {
                let mut parser = ExpressionParser::new();
                let sequence_token: Token = sequence(&[literal("a")]);
                let captured_chars: &Vec<char> = &to_char_vec("{2,&2}");
                let result_token = parser.parse_repetition(sequence_token, captured_chars, 0);
                assert!(result_token.is_err());
            }

            #[test]
            fn error_on_invalid_range_bracketing() {
                let mut parser = ExpressionParser::new();
                let sequence_token: Token = sequence(&[literal("a")]);
                let captured_chars: &Vec<char> = &to_char_vec("{2,(4))+1}");
                let result_token = parser.parse_repetition(sequence_token, captured_chars, 0);
                assert!(result_token.is_err());
            }

            #[test]
            fn error_on_missing_range_enclosure() {
                let mut parser = ExpressionParser::new();
                let sequence_token: Token = sequence(&[literal("a")]);
                let captured_chars: &Vec<char> = &to_char_vec("{2,2");
                let result_token = parser.parse_repetition(sequence_token, captured_chars, 0);
                assert!(result_token.is_err()); 
            }

            #[test]
            fn error_on_invalid_range_bracketing_after_expression() {
                let mut parser = ExpressionParser::new();
                let sequence_token: Token = sequence(&[literal("a")]);
                let captured_chars: &Vec<char> = &to_char_vec("{2,2(}");
                let result_token = parser.parse_repetition(sequence_token, captured_chars, 0);
                assert!(result_token.is_err()); 
            }
        }

        mod general_parser {
            use super::*;

            fn assert_tokenised_expression_matches_expectation(expression: &str, expected_token: Token) {
                let output_token = ExpressionParser::produce_token(expression.into()).unwrap().token;
                assert!(output_token == expected_token, "\nExpected:\n{}\nGot:\n{}", ExpressionParser::get_string(&expected_token), ExpressionParser::get_string(&output_token));
            }

            fn assert_tokenised_expression_produces_error(expression: &str) {
                let output_token = ExpressionParser::produce_token(expression.into());
                assert!(output_token.is_err(), "Invalid expression '{}' incorrectly parsed as valid\nExpression tree:\n{}", expression, ExpressionParser::get_string(&output_token.unwrap().token));
            }

            #[test]
            fn parser_generates_expected_choice_from_string() {
                let expected_token = choice(&[
                    literal("abc"),
                    literal("def"),
                    literal("ghi")
                ]);
                assert_tokenised_expression_matches_expectation("abc|def|ghi", expected_token);
            }

            #[test]
            fn parser_generates_expected_complex_sequence_from_string() {
                let expected_token = sequence(&[
                    literal("a"),
                    choice(&[
                        literal("ab"),
                        literal("cd")
                    ]),
                    choice(&[
                        literal("a"),
                        literal("b")
                    ]),
                    literal("b")
                ]);
                assert_tokenised_expression_matches_expectation("a(ab|cd)(a|b)b", expected_token);
            }

            // significantly more testing is done for the repetition parser itself - this is moreso an integration test
            #[test]
            fn parser_generates_expected_simple_literal_range() {
                let expected_token = repetition(
                    literal("a"),
                    Bound::Literal(1),
                    Bound::Literal(5)
                );
                assert_tokenised_expression_matches_expectation("a{1,5}", expected_token);
            }

            #[test]
            fn parser_generates_expected_adjacent_implicit_repetitions() {
                let expected_token = sequence(&[
                    literal("ab"),
                    repetition(literal("a"), Bound::Literal(0), Bound::Literal(1)),
                    repetition(literal("b"), Bound::Literal(0), Bound::Literal(1)),
                    repetition(literal("c"), Bound::Literal(0), Bound::Literal(1)),
                    literal("cd")
                ]);
                assert_tokenised_expression_matches_expectation("ab(a?)(b?)(c?)cd", expected_token);
            }

            #[test]
            fn parser_handles_excessively_bracketed_repetition() {
                let expected_token = repetition(
                    literal("a"),
                    Bound::Literal(0),
                    Bound::Literal(1)
                );
                assert_tokenised_expression_matches_expectation("(((((a))?)))", expected_token);
            }

            #[test]
            fn parser_generates_expected_heavily_nested_sequence() {
                let expected_token = sequence(&[
                    literal("ab"),
                    repetition(
                        sequence(&[
                            literal("a"),
                            repetition(literal("c"), Bound::Literal(1), Bound::Literal(3)),
                            literal("a")
                        ]
                    ), Bound::Literal(0), Bound::Literal(1))
                ]);
                assert_tokenised_expression_matches_expectation("ab((a(c{1,3})a)?)", expected_token);
            }

            #[test]
            fn parser_generates_expected_heavily_nested_diverse_repetition() {
                let expected_token = repetition(
                    repetition(
                        repetition(
                            repetition(
                                repetition(literal("a"), Bound::Literal(0), Bound::Literal(1)
                            ), Bound::Literal(0), Bound::Literal(1)
                        ), Bound::Literal(0), Bound::Literal(REPEAT_LIMIT)
                    ), Bound::Literal(1), Bound::Literal(REPEAT_LIMIT)
                ), Bound::Literal(0), Bound::Literal(1));

                assert_tokenised_expression_matches_expectation("a??*+?", expected_token);
            }

            #[test]
            fn parser_generates_expected_complex_mixed_nested() {
                let expected_token = 
                choice(&[
                    sequence(&[
                        literal("a"),
                        repetition(
                            choice(&[
                                sequence(&[
                                    repetition(literal("a"), Bound::Literal(1), Bound::Literal(2)),
                                    literal("bc")
                                ]),
                                repetition(literal("e"), Bound::Literal(0), Bound::Literal(1)),
                                sequence(&[
                                    literal("gg"),
                                    choice(&[
                                        literal("a"),
                                        literal("b"),
                                        choice(&[
                                            repetition(literal("c"), Bound::Literal(3), Bound::Literal(3)),
                                            repetition(literal("d"), Bound::Literal(1), Bound::Literal(3))
                                        ])
                                    ])
                                ])
                            ]), Bound::Literal(1), Bound::Literal(2)
                        ),
                    ]),
                    literal("abc")
                ]);
                assert_tokenised_expression_matches_expectation("a(a{1,2}bc|e?|gg(a|b|(c{3}|d{1,3}))){1,2}|abc".into(), expected_token);
            }

            #[test]
            fn parser_generates_expected_unclean_valid_input() {
                let expected_token = sequence(&[
                    literal("a"),
                    choice(&[
                        literal("ab"),
                        literal("cd")
                    ]),
                    choice(&[
                        literal("a"),
                        literal("b")
                    ]),
                    literal("b")
                ]);
                assert_tokenised_expression_matches_expectation("   \na\t(ab| c d  ) \n(a | b)\n\tb", expected_token);            
            }
        
            #[test]
            fn parser_errors_on_invalid_range_bounds() {
                assert_tokenised_expression_produces_error("a{}.");
            }

            #[test]
            fn parser_errors_on_invalid_brackets() {
                assert_tokenised_expression_produces_error("((a)()");
            }

            #[test]
            fn parser_errors_on_invalid_choice_split() {
                assert_tokenised_expression_produces_error("|a|b");
            }

            #[test]
            fn parser_errors_on_bracket_conflict() {
                assert_tokenised_expression_produces_error("(a{)a,b}");
            }

            #[test]
            fn parser_errors_on_invalid_range_position() {
                assert_tokenised_expression_produces_error("?a");
            }

            #[test]
            fn parser_errors_on_invalid_parenthesised_range() {
                assert_tokenised_expression_produces_error("a({a, b})");
            }
        }
    }

    // the generator is much more linear than the parser and works with only already formed
    // expression abstract syntax trees - not direct user input.
    mod generator_tests {
        use super::*;

        fn create_context(token: Token, variables: &[(&str, i32)]) -> ContextToken {
            let mut context = HashMap::new();
            for (name, value) in variables {
                context.insert(name.to_string(), *value);
            }
            ContextToken { token, context }
        }

        #[test]
        fn generator_prunes_impossibly_high_target() {
            let token = sequence(&[
                repetition(literal("a"), Bound::Literal(1), Bound::Literal(3)),
                repetition(literal("b"), Bound::Literal(1), Bound::Literal(3))
            ]);
            let context = create_context(token, &[]);
            let target_length = 7;
            let result = context.generate_exact_length_string(&context.token, &context.context, target_length, target_length);
            assert!(result.is_none());
        }

        #[test]
        fn generator_prunes_impossibly_low_target() {
            let token = sequence(&[
                repetition(literal("a"), Bound::Literal(1), Bound::Literal(3)),
                repetition(literal("b"), Bound::Literal(1), Bound::Literal(3))
            ]);
            let context = create_context(token, &[]);
            let target_length = 1;
            let result = context.generate_exact_length_string(&context.token, &context.context, target_length, target_length);
            assert!(result.is_none());
        }

        #[test]
        fn generator_finds_string_of_correct_length() {
            let token = sequence(&[
                repetition(literal("a"), Bound::Literal(1), Bound::Literal(3)),
                repetition(literal("b"), Bound::Literal(1), Bound::Literal(3))
            ]);
            let context = create_context(token, &[]);
            let target_length = 4;
            let result = context.generate_exact_length_string(&context.token, &context.context, target_length, target_length).unwrap();
            assert_eq!(result.len(), target_length);
        }

        #[test]
        fn generator_finds_correct_simple_string() {
            let token = sequence(&[
                literal("aaaa"),
                repetition(literal("bb"), Bound::Literal(1), Bound::Literal(5))
            ]);
            let context = create_context(token, &[]);
            let target_length = 8;
            let result = context.generate_exact_length_string(&context.token, &context.context, target_length, target_length).unwrap();
            assert_eq!(result, "aaaabbbb".to_string());
        }

        #[test]
        fn variable_solver_produces_random_valid_outputs() {
            let token = sequence(&[
                repetition(literal("a"), Bound::Variable("x".into()), Bound::Variable("x".into())),
                repetition(literal("b"), Bound::Variable("y".into()), Bound::Variable("y".into())),
                repetition(literal("c"), Bound::Variable("z".into()), Bound::Variable("z".into()))
            ]);
            let context = create_context(token, &[("x", 0), ("y", 0), ("z", 0)]);
            let target_length = 12;
            let mut results: HashSet<String> = HashSet::new();
            let dependency_graph = context.get_dependency_graph();

            for _ in 0..10 {
                let valid_variables = context.get_valid_variable_values(target_length, &dependency_graph);
                // the annealing-based solver can fail due to being random - not every iteration may have a solution
                if let Some(generated_string) = context.generate_exact_length_string(&context.token, &valid_variables, target_length, target_length) {
                    results.insert(generated_string);
                }
            }
            // should have at least 3 different results generated
            assert!(results.len() >= 3);
        }

        // technically random whether this finds the correct value - can theoretically fail
        #[test]
        fn variable_solver_finds_correct_value_simple() {
            let token = sequence(&[
                repetition(literal("a"), Bound::Variable("x".into()), Bound::Variable("x".into())),
                repetition(literal("b"), Bound::Variable("x".into()), Bound::Variable("x".into()))
            ]);

            let context = create_context(token, &[("x", 0)]);
            let target_length = 10;

            let dependency_graph = context.get_dependency_graph();
            let value_map = context.get_valid_variable_values(target_length as usize, &dependency_graph);
            
            let expected_x = target_length / 2 as i32;
            assert_eq!(value_map.get("x").unwrap(), &expected_x);
        }

        #[test]
        fn variable_solver_finds_multiple_correct_values_simple() {
            let token = sequence(&[
                repetition(literal("a"), Bound::Variable("x".into()), Bound::Variable("x".into())),
                repetition(literal("b"), Bound::Variable("y".into()), Bound::Variable("y".into())),
                repetition(literal("c"), Bound::Variable("z".into()), Bound::Variable("z".into()))
            ]);

            let context = create_context(token, &[("x", 0), ("y", 0), ("z", 0)]);
            let target_length = 10;

            let dependency_graph = context.get_dependency_graph();
            let value_map = context.get_valid_variable_values(target_length as usize, &dependency_graph);

            let x_value = value_map.get("x").unwrap();
            let y_value = value_map.get("y").unwrap();
            let z_value = value_map.get("z").unwrap();

            assert_eq!(x_value + y_value + z_value, 10);
            assert!(x_value >= &0);
            assert!(y_value >= &0);
            assert!(z_value >= &0);
        }

        #[test]
        fn variable_solver_respects_bounds() {
            let token = sequence(&[
                repetition(literal("a"), Bound::Variable("x".into()), Bound::Literal(4)),
                repetition(literal("b"), Bound::Variable("x".into()), Bound::Literal(4))
            ]);

            let context = create_context(token, &[("x", 0)]);
            let target_length = 10;

            let dependency_graph = context.get_dependency_graph();
            let value_map = context.get_valid_variable_values(target_length as usize, &dependency_graph);
            
            let x_value = value_map.get("x").unwrap();

            // solver should respect bounds, and not find the "correct" x value here
            assert_ne!(x_value, &5);
        }

        #[test]
        fn topological_repair_fixes_invalid_variables() {
            let token = sequence(&[
                repetition(literal("a"), Bound::Variable("x".into()), Bound::Variable("y".into())),
                repetition(literal("b"), Bound::Literal(2), Bound::Variable("x".into()))
            ]);

            // initialised with intentially incorrect variables alongside the 2<=x constraint
            let mut context = create_context(token, &[("x", 1), ("y", 0)]);
            let dependency_graph = context.get_dependency_graph();

            ContextToken::enforce_constraints(&mut context.context, &dependency_graph);

            let x_value = context.context.get("x").unwrap();
            let y_value = context.context.get("y").unwrap();

            assert!(x_value <= y_value);
            assert!(x_value >= &2);
        }
    }
}


mod benchmarks {

}