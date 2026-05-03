use crate::{Program, Step, Transition, TuringMachineError, machine::*, types::MAX_EXECUTION_STEPS};

use std::{collections::{HashMap, HashSet}, f64::consts::E, fmt, hash::Hash, ops::Range, usize, vec};
use rand::{Rng, SeedableRng, rngs::{StdRng, ThreadRng}, seq::{IndexedRandom, SliceRandom}};

// constants related to regex-based string generation
const REPEAT_LIMIT: i32 = 128;
const ESCAPE_CHAR: char = '\\';
const DELIMITER_CHAR: char = ';';

// constants related to GA-based string generation
const POPULATION_SIZE: usize = 50;
const MAX_GENERATIONS: usize = 20;

// token struct for representing regular expressions
// covers all basic regex operations 
// essentially an AST for expanded regex

#[derive(PartialEq, Clone, Eq, Hash, Debug)]
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
#[derive(Debug)]
struct ContextToken {
    token: Token,
    context: HashMap<String, i32>,
}

// mathematical operations - used for calculating discrete repetition `Bound` values
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
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
#[derive(PartialEq, Eq, Hash, Debug)]
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

    // used for debugging with recur_to_string. provides a string representation of a bound
    #[warn(unused)]
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
    fn produce_token(expression: &str) -> Result<ContextToken, String> {
        let mut parser: ExpressionParser = ExpressionParser::new();
        let mut cleaned_vector_expression: Vec<char> = expression.chars().filter(|c| !c.is_whitespace()).collect();
        let output_token: Token = Self::parse_chars(&mut parser, &mut cleaned_vector_expression, '\0', 0)?.0;

        return Ok(ContextToken{token: output_token, context: parser.context});
    }

    /// checks for errors in a given literal, and returns a Token::Literal containing the given `Vec<char>` if none are found
    fn parse_literal(&self, c_vec: Vec<char>, index: usize) -> Result<(Token, usize), String> {
        let min_length: usize = 1;
        let initial_length: usize = c_vec.len();
        // unreachable so long as min_length = 1
        if initial_length < min_length {
            return Err(format!("Error: Received invalid literal - literals must be at least {} character(s) long", min_length))
        }

        for next_char in &c_vec {
            // redundancy - already checked for elsewhere before producing a literal
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
        let special_chars: Vec<char> = vec!['(', '|', '{', '*', '+', '?', ')', '}', ESCAPE_CHAR, exit_char];
        let total_len: usize = c_vec.len();
        let mut token_vec: Vec<Token> = Vec::new();
        let mut next_char: char = c_vec[index];
        let mut literal_buffer: Vec<char> = Vec::new();
        let mut choice_indices: Vec<usize> = vec![0];

        while next_char != exit_char {
            match next_char {
                // escape character - forces next character to be evaluated as a literal
                ESCAPE_CHAR => {
                    if let Some(escaped_char) = c_vec.get(index + 1) {
                        literal_buffer.push(*escaped_char);
                    } else {
                        return Err(format!("Error: Received invalid escape at index {} - escape must have a target", index));
                    }
                    index += 1;
                }
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

    fn get_string(token: &Token) -> String {
        return Self::recur_to_string(&token, 0);
    }

    // converts a given token into its string representation - used for debug
    #[warn(unused)]
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
    fn generate_strings_in_range(&self, search_range: Range<usize>, max_generations: usize) -> Vec<String> {
        let mut rng = rand::rng();
        self.generate_seeded_strings_in_range(search_range, max_generations, &mut rng)
    }

    // generates strings within the search range length, potentially stopping early if max_generations has been exceeded
    fn generate_seeded_strings_in_range<T: Rng>(&self, search_range: Range<usize>, max_generations: usize, rng: &mut T) -> Vec<String> {
        let mut generated_strings: Vec<String> = Vec::new();
        let mut lengths_generated: HashSet<usize> = HashSet::new();

        // used to ensure that repairs to parameters are done in the correct order such that
        // they will always generate a non-contradicting set of output variables  
        let dependency_graph = self.get_dependency_graph();

        for target_length in search_range.clone() {
            let valid_vars = self.get_valid_variable_values(target_length, &dependency_graph, rng);
            let generated_string = self.generate_string_to_length(&self.token, &valid_vars, target_length.clone(), rng);
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
            if lengths_generated.len() >= max_generations {
                break;
            }
        }
        // returns strings in length order
        generated_strings.sort_by_key(|s| s.len());

        return generated_strings;
    }

    // get all lengths possible by each token (get_possible_token_lengths, recursive DP-based generator)
    // reject instantly if this is not possible
    // or generate through backtracking from possible lengths. should always produce a correct string of length n
    fn generate_string_to_length<T: Rng>(&self, token: &Token, variables: &HashMap<String, i32>, target_length: usize, rng: &mut T) -> Option<String> {
        // gets all token lengths for this input and builds the length HashSet cache
        let possible_lengths=  self.get_possible_token_lengths(token, variables, target_length);
        if !possible_lengths.contains(&target_length) {
            return None;
        }
        
        self.generate_exact_length_string(token, variables, target_length, target_length, rng)
    }

    fn generate_exact_length_string<T: Rng>(&self, token: &Token, variables: &HashMap<String, i32>, target_length: usize, length_limit: usize, rng: &mut T) -> Option<String> {
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

                // ensures that a single failure will not cause generation to be abandoned
                valid_choices.shuffle(rng);
                for choice in valid_choices {
                    if let Some(generated_string) = self.generate_exact_length_string(choice, variables, target_length, length_limit, rng) {
                        return Some(generated_string);
                    }
                }
                None
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
                valid_repetitions.shuffle(rng);

                for repetition_count in valid_repetitions {
                    // form a sequence token instead, have this be solved by the sequence solver
                    let sequence_token = Token::Sequence(vec![repetition_token.as_ref().clone(); repetition_count]);
                    if let Some(valid_string) = self.generate_exact_length_string(&sequence_token, variables, target_length, length_limit, rng) {
                        return Some(valid_string);
                    }
                }
                None
            },
            Token::Sequence(tokens) => {
                self.generate_valid_sequence_partition(tokens, variables, 0, target_length, length_limit, rng)
            }
        }
    }

    fn generate_valid_sequence_partition<T: Rng>(&self, tokens: &[Token], variables: &HashMap<String, i32>, index: usize, remaining_length: usize, length_limit: usize, mut rng: &mut T) -> Option<String> {
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

        // requires sorting before shuffle to ensure that the hashmap is ordered deterministically
        // without this, the seed is essentially ignored
        possible_token_lengths.sort_unstable();
        possible_token_lengths.shuffle(rng);

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
            if let Some(partition_remainder) = self.generate_valid_sequence_partition(tokens, variables, index + 1, partition_length, length_limit, rng) {
                // if the rest of the partition can be valid, generate the current part
                if let Some(generated_partition) = self.generate_exact_length_string(current_token, variables, possible_length, length_limit, rng) {
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
    fn get_valid_variable_values<T: Rng>(&self, target_length: usize, dependency_graph: &DependencyGraph, mut rng: &mut T) -> HashMap<String, i32> {
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
            Self::mutate_variable(&mut mutated_vars, &dependency_graph.order, rng);
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
    fn mutate_variable<T: Rng>(variables: &mut HashMap<String, i32>, names: &Vec<String>, rng: &mut T) {
        let distribute_randomly = | generators: i32, value_range: i32, rng: &mut T | {
            let mut sum = 0;
            for _ in 0..generators { sum += rng.random_range(0..value_range); }
            return sum - ((value_range - 1) * generators) / 2;
        };

        if names.is_empty() {
            return;
        }

        let target_var = names.get(rng.random_range(0..names.len())).unwrap();
        let target_value = variables.get_mut(target_var).unwrap();

        // apply a random 'mutation', modifying the variable value
        // essentially uses dice rolls to cheaply simulate an inverse normal distribution
        *target_value = (*target_value + distribute_randomly(3, 7, rng) / 2).max(0);
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

        // ensures determinism of hashmaps so seeds are not ignored
        let mut sorted_elements: Vec<String> = elements.into_iter().collect();
        sorted_elements.sort();

        // some elements will not work with this, as this algorithm assumes a DAG is input
        // therefore, for cycles, an arbitrary order must be given
        for cyclic_element in sorted_elements {
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

struct RuntimeInfo {
    pub graph_data: Vec<(usize, usize)>,
    pub state_graph_data: HashMap<String, Vec<(usize, usize)>>
}

pub struct AnalysisInfo {
    pub estimated_complexity: Complexity,
    pub graph_data: Vec<(usize, usize)>,
    pub estimated_state_complexities: HashMap<String, Complexity>,
    pub state_graph_data: HashMap<String, Vec<(usize, usize)>>
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Copy)]
// could add more. each just needs a corresponding complexity function definition, but the more there are the higher the chance of an incorrect classification
// due to the impact of lesser terms
pub enum Complexity {
    Unknown = isize::MIN,
    Constant = 0,
    N = 10,
    Nlogn = 20,
    N2 = 30,
    N2logn = 40,
    N3 = 50,
    Exp = 60,
}

impl Complexity {
    fn get_complexity_function(&self) -> Box<dyn Fn(usize) -> f64> {
        match self {
            Complexity::Constant => Box::new(| _: usize | 1f64),
            Complexity::N => Box::new(| n: usize | n as f64),
            Complexity::Nlogn => Box::new(| n: usize | {
                let x: f64 = n as f64;
                x * x.log2()
            }),
            Complexity::N2 => Box::new(| n: usize | (n.pow(2)) as f64),
            Complexity::N2logn => Box::new(| n: usize | {
                let x: f64 = n as f64;
                x.powi(2) * x.log2()
            }),
            Complexity::N3 => Box::new(| n: usize | (n.pow(3)) as f64),
            Complexity::Exp => Box::new(| n: usize | E.powi(n as i32)),
            _ => Box::new(| _: usize| 0f64 ),
        }
    }
}

impl fmt::Display for Complexity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        return match self {
            Complexity::Constant => write!(f, "O(1)"),
            Complexity::N => write!(f, "O(n)"),
            Complexity::N2 => write!(f, "O(n²)"),
            Complexity::Nlogn => write!(f, "O(n log n)"),
            Complexity::N2logn => write!(f, "O(n² log n)"),
            Complexity::N3 => write!(f, "O(n³)"),
            Complexity::Exp => write!(f, "O(2ⁿ)"),
            _ => write!(f, "None"),
        }
    }
}

fn process_expression_string(expression_string: &str) -> Result<Vec<String>, String> {
    let mut expression_vec: Vec<String> = Vec::new();
    let mut expression_buffer: Vec<char> = Vec::new();
    let mut escaped_char = false;

    for (index, next_char) in expression_string.chars().enumerate() {
        if escaped_char {
            expression_buffer.push(next_char);
            escaped_char = false;
        } else if next_char == ESCAPE_CHAR {
            escaped_char = true;
            expression_buffer.push(next_char);
        } else if next_char == DELIMITER_CHAR {
            if let Some(error_message) = push_buffer(&mut expression_buffer, &mut expression_vec, index) { 
                return Err(error_message);
            }
        } else {
            expression_buffer.push(next_char);
        }
    }

    if let Some(error_message) = push_buffer(&mut expression_buffer, &mut expression_vec, expression_string.len()) { 
        return Err(error_message);
    }

    Ok(expression_vec)
}

// pushes the expression buffer onto the expression vec, and clears the buffer
// returns a String if there is an error, otherwise returns None
fn push_buffer(expression_buffer: &mut Vec<char>, expression_vec: &mut Vec<String>, index: usize) -> Option<String> {
    if expression_buffer.len() == 0 { return Some(format!("Error: Empty expression in expression string at index {}", index)) }
    let buffer_string: String = expression_buffer.iter().collect();
    expression_vec.push(buffer_string);

    expression_buffer.clear();
    return None;
}

fn compile_analysis(analysis_data: Vec<RuntimeInfo>) -> AnalysisInfo {
    let mut final_analysis_info = AnalysisInfo {
        estimated_complexity: Complexity::Unknown,
        graph_data: Vec::new(),
        estimated_state_complexities: HashMap::new(),
        state_graph_data: HashMap::new(),
    };

    // add all worst step counts to a central map - cleans data, removes duplicates for a given length
    let mut final_graph_map: HashMap<usize, usize> = HashMap::new();
    for analysed_expression in &analysis_data {
        for &(input_length, steps) in &analysed_expression.graph_data {
            final_graph_map
                .entry(input_length)
                .and_modify(|current_length| *current_length = steps.max(*current_length))
                .or_insert(steps);
        }
    }

    let mut final_graph: Vec<(usize, usize)> = final_graph_map.into_iter().collect();
    final_graph.sort_by_key(|&(input_length, _)| input_length);

    final_analysis_info.graph_data = final_graph;
    final_analysis_info.estimated_complexity = estimate_complexity(&mut final_analysis_info.graph_data);

    let mut final_graph_maps: HashMap<String, HashMap<usize, usize>> = HashMap::new();
    // compile state complexities for each state capped by highest estimated complexity
    for analysed_expression in analysis_data {
        for (state, mut graph_data) in analysed_expression.state_graph_data {
            let estimated_complexity = estimate_complexity(&mut graph_data);
            final_analysis_info.estimated_state_complexities
                .entry(state.clone())
                .and_modify(|current_complexity | *current_complexity = final_analysis_info.estimated_complexity.min(estimated_complexity))
                .or_insert(Complexity::Unknown);
            
            let graph_map = final_graph_maps.entry(state).or_insert_with(|| HashMap::new());
            for (input_length, steps) in graph_data {
                graph_map
                    .entry(input_length)
                    .and_modify(|current_steps| *current_steps = steps.max(*current_steps))
                    .or_insert(steps);
            }
        }
    }

    // orders state length/input size pairs for graphing
    for (state, graph_map) in final_graph_maps {
        let mut state_graph: Vec<(usize, usize)> = graph_map.into_iter().collect();
        state_graph.sort_by_key(|&(input_length, _)| input_length);
        final_analysis_info.state_graph_data.insert(state, state_graph);
    }

    return final_analysis_info;
}

pub fn analyse_expression(expression_string: &str, program: &Program, strict: bool, attempts: usize) -> Result<AnalysisInfo, String> {
    let max_generation_length = 100;
    let max_generations = 50;

    let expression_strings = process_expression_string(expression_string)?;
    let mut analysis_data: Vec<RuntimeInfo> = Vec::new();

    for expression in expression_strings {
        let token = ExpressionParser::produce_token(&expression)?;
        let mut generated_inputs = Vec::new();

        for _ in 0..attempts {
            generated_inputs.extend(token.generate_strings_in_range(0..max_generation_length, max_generations));
        }

        let runtime_info: RuntimeInfo = evaluate_inputs(generated_inputs, program, strict);
        if runtime_info.graph_data.is_empty() {
            return Err(format!("Error: Failed to simulate Turing Machine for expression '{}' - no valid outputs found", expression));
        }

        analysis_data.push(runtime_info);
    }

    Ok(compile_analysis(analysis_data))
}

fn estimate_complexity(machine_points: &mut Vec<(usize, usize)>) -> Complexity {
    machine_points.sort_by_key(|&(index, _)| index);

    let complexities: Vec<Complexity> = vec![Complexity::Constant, Complexity::N, Complexity::Nlogn, Complexity::N2, Complexity::N2logn, Complexity::N3, Complexity::Exp];
    let mut best_complexity = Complexity::Constant;
    let mut best_error = f64::MAX;

    for complexity in complexities {
        let complexity_function = complexity.get_complexity_function();

        // calculates constant multiplier which minimises error (b)
        let mut xy: f64 = 0f64;
        let mut xx: f64 = 0f64;
        for &(index, value) in machine_points.iter() {
            let x = complexity_function(index.clone());
            let y = value as f64;
            // skip iterations where the state is never visited, or has hit the maximum number of steps
            if value == 0 || value == MAX_EXECUTION_STEPS {
                continue;
            }

            xy += x * y;
            xx += x * x;
        }

        // if there are no remaining valid states, exit before continuing further
        if xx == 0f64 {
            return Complexity::Unknown;
        }

        // get least squares constant estimation through B = (X * Y) / (X * X)
        let b = xy / xx;

        // now calculates residual error by using the constant multiplier. the complexity with the lowest adjusted error is the most likely complexity
        let mut residual_sum: f64 = 0f64;
        for &(index, value) in machine_points.iter() {
            // standard rss * weighting - higher indices are more trustworthy as higher time complexities are more prominent as n tends to infinity
            residual_sum += (value as f64 - b * complexity_function(index)).powi(2) * ((index as f64).sqrt());
        }

        if residual_sum < best_error {
            best_error = residual_sum;
            best_complexity = complexity;
        }
    }

    best_complexity
}

fn extract_alphabet(program: &Program) -> Result<Vec<char>, String> {
    let mut alphabet: Vec<char> = program.rules.values()
        .flat_map(|transition| transition.iter().flat_map(|t| t.read.clone()))
        .filter(|&c| c != program.blank)
        .collect::<HashSet<char>>()
        .into_iter()
        .collect::<Vec<char>>();

    alphabet.sort();

    if alphabet.is_empty() {
        return Err("Error: Could not find valid alphabet from provided Turing Machine".into());
    }
    Ok(alphabet)
}

pub fn analyse_automatic(program: &Program, max_length: usize, strict: bool, attempts: usize) -> Result<AnalysisInfo, String> {
    // produces an alphabet from transitions to use as chromosones in GA
    let inferred_alphabet: Vec<char> = extract_alphabet(program)?;

    let mut best_inputs = Vec::new();
    for length in 1..=max_length {
        let best_input = generate_genetically(program, length, strict, &inferred_alphabet);
        if !best_input.is_empty() {
            best_inputs.push(best_input);
        }
    }

    let runtime_info = evaluate_inputs(best_inputs, program, strict);

    Ok(compile_analysis(vec![runtime_info]))
}

fn evaluate_inputs(inputs: Vec<String>, program: &Program, strict: bool) -> RuntimeInfo {
    let mut total_runtimes = Vec::new();
    let mut state_runtimes_map: HashMap<String, Vec<(usize, usize)>> = HashMap::new();

    for input in inputs {
        let mut machine = TuringMachine::new(program.clone());
        if machine.set_tape_content(0, &input).is_err() {
            continue;
        }

        machine.run();

        if strict && !machine.state().to_lowercase().contains("accept") {
            continue;
        }

        let length = input.len();
        let steps = machine.step_count();

        total_runtimes.push((length, steps));
        let transition_steps = machine.get_transition_steps();
        for state_name in program.rules.keys() {
            let state_steps = transition_steps.get(state_name).copied().unwrap_or(0);
            state_runtimes_map.entry(state_name.clone())
                .or_default()
                .push((length, state_steps));
        }
    }

    RuntimeInfo { 
        graph_data: total_runtimes, 
        state_graph_data: state_runtimes_map 
    }
}

fn generate_genetically(program: &Program, length: usize, strict: bool, alphabet: &Vec<char>) -> String {
    let mut rng = rand::rng();

    let mut population: Vec<String> = (0..POPULATION_SIZE)
        .map(|_| (0..length).map(|_| *alphabet.choose(&mut rng).unwrap()).collect())
        .collect();

    let mut current_best_string: String = String::new();
    let mut current_best_fitness: usize = 0;

    for _ in 0..MAX_GENERATIONS {
        // evaluate fitness of current generation
        let mut evaluation_tuple: Vec<(String, usize)> = population.into_iter().map(|s| {
            let fitness = calculate_fitness(program, &s, strict);
            (s, fitness)
        }).collect();

        evaluation_tuple.sort_by_key(|&(_, fitness)| fitness);

        // update best string seen so far
        let generation_best_string = evaluation_tuple.last().unwrap();
        if generation_best_string.1 > current_best_fitness {
            current_best_fitness = generation_best_string.1;
            current_best_string = generation_best_string.0.clone();
        }

        // simulate suvival of the fittest (keeps top 50%)
        let survivors = &evaluation_tuple[POPULATION_SIZE / 2..];
        // always preserves the best string from the previous generation
        let mut next_generation = vec![generation_best_string.0.clone()];

        while next_generation.len() < POPULATION_SIZE {
            // chooses 2 random distinct "parents"
            let parents: Vec<&(String, usize)> = survivors.choose_multiple(&mut rng, 2).collect();

            let parent_a = &parents[0].0;
            let parent_b = &parents[1].0;

            let crossover_point = rng.random_range(0..=length);
            let mut child = format!("{}{}", &parent_a[..crossover_point], &parent_b[crossover_point..]);

            // 10% base mutation chance
            if length > 0 && rng.random_bool(0.1) {
                let mutation_point = rng.random_range(0..length);
                let new_char = *alphabet.choose(&mut rng).unwrap();
                child.replace_range(mutation_point..=mutation_point, &new_char.to_string());
            }

            next_generation.push(child);
        }

        population = next_generation;
    }

    current_best_string
}

fn calculate_fitness(program: &Program, input: &str, strict: bool) -> usize {
    let mut machine = TuringMachine::new(program.clone());
    // if input is invalid, disqualify
    if machine.set_tape_content(0, input).is_err() { return 0; }

    machine.run();

    // if mode is strict and this input was not accepted, disqualify
    if strict && !machine.state().to_lowercase().contains("accept") { return 0; }

    // otherwise, return the step count as the fitness value
    machine.step_count()
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
            let result = context.generate_exact_length_string(&context.token, &context.context, target_length, target_length, &mut StdRng::seed_from_u64(0u64));
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
            let mut rng = StdRng::seed_from_u64(0);
            let result = context.generate_exact_length_string(&context.token, &context.context, target_length, target_length, &mut rng);
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
            let mut rng = StdRng::seed_from_u64(0);
            let result = context.generate_exact_length_string(&context.token, &context.context, target_length, target_length, &mut rng).unwrap();
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
            let mut rng = StdRng::seed_from_u64(0);
            let result = context.generate_exact_length_string(&context.token, &context.context, target_length, target_length, &mut rng).unwrap();
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

            let mut rng = StdRng::seed_from_u64(0);
            for _ in 0..10 {
                let valid_variables = context.get_valid_variable_values(target_length, &dependency_graph, &mut rng);
                // the annealing-based solver can fail due to being random - not every iteration may have a solution
                if let Some(generated_string) = context.generate_exact_length_string(&context.token, &valid_variables, target_length, target_length, &mut rng) {
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
            let mut rng = StdRng::seed_from_u64(0);
            let value_map = context.get_valid_variable_values(target_length as usize, &dependency_graph, &mut rng);
            
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
            let mut rng = StdRng::seed_from_u64(0);
            let value_map = context.get_valid_variable_values(target_length as usize, &dependency_graph, &mut rng);

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
            let mut rng = StdRng::seed_from_u64(0);
            let value_map = context.get_valid_variable_values(target_length as usize, &dependency_graph, &mut rng);
            
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
    
        #[test]
        fn generator_can_generate_simple_odd_range() {
            let token = sequence(&[
                repetition(literal("aa"), Bound::Variable("x".into()), Bound::Variable("x".into())),
                repetition(literal("bb"), Bound::Variable("y".into()), Bound::Variable("y".into())),
                literal("ccccc")
            ]);
            let context = create_context(token, &[("x", 0), ("y", 0)]);
            let range = 0..20;
            let generated_strings= context.generate_strings_in_range(range, 100);
            // expects strings of length 5 + 2n: 8 in range 0..20
            assert!(generated_strings.len() == 8);
        }

        #[test]
        fn generator_can_generate_complex_literal_range() {
            let token =
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

            let context = create_context(token, &[]);
            let range = 0..40;
            let generated_strings= context.generate_strings_in_range(range, 100);

            assert!(generated_strings.len() == 11);
        }

        #[test]
        fn generator_can_generate_complex_mixed_range() {
            let token =
            choice(&[
                sequence(&[
                    literal("a"),
                    repetition(
                        choice(&[
                            sequence(&[
                                repetition(literal("a"), Bound::Variable("x".into()), Bound::Literal(4)),
                                literal("bc")
                            ]),
                            repetition(literal("e"), Bound::Variable("x".into()), Bound::Variable("y".into())),
                            sequence(&[
                                literal("gg"),
                                choice(&[
                                    literal("a"),
                                    literal("b"),
                                    choice(&[
                                        repetition(literal("c"), Bound::Literal(1), Bound::Variable("z".into())),
                                        repetition(literal("d"), Bound::Variable("x".into()), Bound::Calculation(Box::new(Bound::Literal(2)), Operation::Add, Box::new(Bound::Variable("z".into()))))
                                    ])
                                ])
                            ])
                        ]), Bound::Literal(1), Bound::Literal(2)
                    ),
                ]),
                literal("abc")
            ]);

            let context = create_context(token, &[("x", 0), ("y", 0), ("z", 0)]);
            let range = 0..40;
            let generated_strings= context.generate_strings_in_range(range, 100);
            
            // able to generate all lengths but length 2
            assert!(generated_strings.len() == 38);
        }
    }
}

mod benchmarks {
    use super::*;
    use std::time::Instant;

    fn benchmark_input(input_str: &str, generation_range: Range<usize>, benchmark_name: &str) {
        let max_generations = 100;
        let mut rng = StdRng::seed_from_u64(0);

        let start = Instant::now();
        let token = ExpressionParser::produce_token(input_str).unwrap();
        let generated_strings = token.generate_seeded_strings_in_range(generation_range.clone(), max_generations, &mut rng);
        let elapsed = Instant::now() - start;

        println!("Benchmark '{}'\nInput String: {}\nTime Elapsed: {:<7?}\nSearch Range: {}..{}\nStrings Generated: {}\n", benchmark_name, input_str, elapsed, generation_range.start, generation_range.end, generated_strings.len())
    }

    #[test]
    fn simple_string() {
        benchmark_input("a{,}", 0..100, "Simple Linear Regex");
    }

    #[test]
    fn choice_nonatomic_string() {
        benchmark_input("(aaa|bbbbb){,}", 0..50, "Simple Non-Atomic String");
    }

    #[test]
    fn simple_variable_bound_string() {
        benchmark_input("a{x}b{x}", 0..50, "Simple Variable-Bounded String");
    }

    #[test]
    fn complex_variable_bound_string() {
        benchmark_input("(a{3x, y + 5 + z - x}(b|c){y, 6x}){x - z}", 0..50, "Complex Variable-Bounded String");
    }

    #[test]
    fn complex_choice_string() {
        benchmark_input("((((a|b|c|d|e){5})|fff){2})|g{, 10}", 0..50, "Complex Choice String");
    }
}