use std::{collections::{HashMap, HashSet}, usize, vec, ops::Range};
use rand::{Rng, rng, rngs::ThreadRng, seq::IndexedRandom};

const REPEAT_LIMIT: i32 = 256;

// token struct for representing regular expressions
// covers all basic regex operations 
// essentially an AST for expanded regex

#[derive(PartialEq)]
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
#[derive(Clone, PartialEq)]
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
#[derive(PartialEq)]
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

impl ContextToken {
    /// potential solutions:
    ///     annealing - simplified GE which probabilistically finds a global maximum using a gradient descent-like algorithmic search with mutations
    ///     pros - simpler, performant way to estimate a solution
    ///     cons - may struggle with complex cases with many variables
    /// 
    ///     genetic algorithm - genetic search using variables as genes, maintaining a population over generations and gradually getting closer to a correct result
    ///     pros - much more likely to find a correct answer, especially in complex cases with many variables. can also find many solutions simultaneously
    ///     cons - much slower - may still struggle with complex cases with strangely shaped search spaces
    /// 
    /// both will require some methodology to enforce variable constraints - can be added to fitness function of a GE
    /// constraints should be handled using a dependency graph, such that constraints are enforcable
    /// 
    ///     after this, the variables generated can be fed into another search algorithm to find a valid string based on literal-based ranges
    /// 
    ///     https://en.wikipedia.org/wiki/Simulated_annealing
    ///     https://en.wikipedia.org/wiki/Genetic_algorithm
    ///     https://en.wikipedia.org/wiki/Dependency_graph
    ///     https://en.wikipedia.org/wiki/Topological_sorting#Kahn's_algorithm
    /// 
    /// end solution also needs a way to generate strings from these chosen variables
    /// this could use a greedy generator with a budget of n (where n is the target length)
    /// picking an option reduces the budget recursively until no more options are availible
    /// could alternatively use a more intelligent algorithm to ensure a solution will always be found if it exists 

    pub fn generate_to_length(&self, search_range: Range<usize>) -> Vec<String> {
        let mut generated_strings: Vec<String> = Vec::new();
        let mut lengths_generated: HashSet<usize> = HashSet::new();

        // used to ensure that repairs to parameters are done in the correct order such that
        // they will always generate a non-contradicting set of output variables  
        let dependency_graph = self.get_dependency_graph();

        for target_length in search_range.clone() {
            self.get_valid_variables(target_length, &dependency_graph);
            let generated_string = self.generate_string(target_length);
            let generated_length = generated_string.len();

            if !lengths_generated.contains(&generated_length) && search_range.contains(&generated_length) {
                generated_strings.push(generated_string);
                lengths_generated.insert(generated_length);
            }
        }
        generated_strings.sort_by_key(|s| s.len());

        return generated_strings;
    }

    fn generate_string(&self, target_length: usize) -> String {
        return String::new();
        // TODO: finish
    }

    /// uses an annealing-based approach to find valid variables based on constraints
    fn get_valid_variables(&self, target_length: usize, dependency_graph: &DependencyGraph) -> HashMap<String, i32> {
        // for random mutations
        let mut rng = rand::rng();
        let mut var_state = self.context.clone();
        Self::enforce_constraints(&mut var_state, &dependency_graph);

        let mut best_length = self.calculate_max_length(&self.token, &var_state);
        let mut best_vars = var_state.clone();
        let mut best_diff = target_length.abs_diff(best_length);

        let max_iterations = 500;
        let init_temp = target_length as f64;

        for i in 0..max_iterations {
            if best_diff == 0 {
                return best_vars;
            }

            let temp = init_temp * (1f64 - i as f64 / max_iterations as f64);
            let mut mutated_vars = var_state.clone();
            self.mutate_variable(&mut mutated_vars, &dependency_graph.order, &mut rng);
            Self::enforce_constraints(&mut mutated_vars, &dependency_graph);
        }
        // TODO: finish
        return HashMap::new();
    }

    fn calculate_max_length(&self, analysed_token: &Token, context: &HashMap<String, i32>) -> usize {
        return match analysed_token {
            Token::Literal(literal) => literal.len(),
            Token::Repetition(repeated_token, _, upper_bound) => self.calculate_max_length(repeated_token.as_ref(), &context) * Bound::calculate_bound(upper_bound, &self.context).unwrap_or(0) as usize,
            Token::Choice(token_vec) => token_vec.iter().map(| token | self.calculate_max_length(token, &context)).max().unwrap_or(0),
            Token::Sequence(token_vec) => token_vec.iter().map(| token | self.calculate_max_length(token, &context)).sum(),
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
        // for each variable, ensures that all constraints are fulfilled
        for var in &dependency_graph.order {
            let mut var_value: i32 = 0;
            for constraint in &dependency_graph.constraints {
                // oversimplification - assumes a max bound is equal to the variable used
                // should work for simple cases, but complex cases may break this logic
                if constraint.max.has_variable(var) {
                    let min_value: i32 = constraint.min.calculate_bound(variables).unwrap_or(0);
                    var_value = var_value.max(min_value);
                }
            }

            let current_var = *variables.get(var).unwrap_or(&0);
            if current_var < var_value as i32 {
                variables.insert(var.clone(), var_value);
            }
        }
    }

    fn mutate_variable(&self, variables: &mut HashMap<String, i32>, names: &Vec<String>, rng: &mut impl Rng) {
        if names.is_empty() {
            return;
        }

        // get a random variable to mutate
        let target_var = names.get(rng.random_range(0..names.len())).unwrap();
        let target_value = variables.get_mut(target_var).unwrap();

        // apply a random 'mutation', modifying the variable value
        // this may require later tuning - these values are not well tested
        *target_value = (*target_value + match rng.random_range(0..100) {
            0..60 => rng.random_range(-1..=1),
            60..85 => rng.random_range(-4..=4),
            _ => rng.random_range(-10..=10),
        }).max(0);
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


struct Expression { 
    c_token: ContextToken,
    min_length: usize,
}

// deprecated, ContextToken now used instead to generate strings
impl Expression {
    pub fn from_token(&self, expression_token: ContextToken) -> Result<Expression, String> {
        let min_gen_length: usize = self.calculate_min_length(&expression_token.token);
        return Ok(Expression { c_token: expression_token, min_length: min_gen_length });
    }

    pub fn from_string(&self, expression_string: &str) -> Result<Expression, String> {
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
                let context: &HashMap<String, i32> = &self.c_token.context;

                let inner_token: &Token = token.as_ref();
                let lower: usize = Bound::calculate_bound(lower_bound, context).ok_or("Negative lower bound length".to_string())? as usize;
                let upper: usize = Bound::calculate_bound(upper_bound, context).ok_or("Negative upper bound length".to_string())? as usize;
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
            Token::Literal(lit) => Bound::Literal(lit.len() as i32),
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
            Token::Literal(lit) => Bound::Literal(lit.len() as i32),
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
            Token::Repetition(repeated_token, lower_bound, _) => self.calculate_min_length(repeated_token.as_ref()) * Bound::calculate_bound(lower_bound, &self.c_token.context).unwrap_or(0) as usize,
            Token::Choice(token_vec) => token_vec.iter().map(| token | self.calculate_min_length(token)).min().unwrap_or(0),
            Token::Sequence(token_vec) => token_vec.iter().map(| token | self.calculate_min_length(token)).sum(),
        };
    }

    fn calculate_max_length(&self, analysed_token: &Token) -> usize {
        return match analysed_token {
            Token::Literal(literal) => literal.len(),
            Token::Repetition(repeated_token, _, upper_bound) => self.calculate_max_length(repeated_token.as_ref()) * Bound::calculate_bound(upper_bound, &self.c_token.context).unwrap_or(0) as usize,
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
    let output_token: Result<ContextToken, String> = ExpressionParser::produce_token("(ab|cd|(e{x*2,}|a{,4})|f){,12}a");
    match output_token {
        Ok(tk) => println!("{}", ExpressionParser::get_string(&tk.token)),
        Err(er) => println!("{}", er),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // bound tests
    mod bounds {
        use super::*;

        #[test]
        fn test_bound_mult() {
            let ctx: HashMap<String, i32> = HashMap::new();
            let test_calculation: Bound = Bound::Calculation(Box::new(Bound::Literal(5)), Operation::Multiply, Box::new(Bound::Literal(10)));
            let result = Bound::calculate_bound(&test_calculation, &ctx);
            assert_eq!(result.unwrap(), 50);
        }

        #[test]
        fn test_bound_add() {
            let ctx: HashMap<String, i32> = HashMap::new();
            let test_calculation: Bound = Bound::Calculation(Box::new(Bound::Literal(5)), Operation::Add, Box::new(Bound::Literal(10)));
            let result = Bound::calculate_bound(&test_calculation, &ctx);
            assert_eq!(result.unwrap(), 15);
        }

        #[test]
        fn test_bound_sub() {
            let ctx: HashMap<String, i32> = HashMap::new();
            let test_calculation: Bound = Bound::Calculation(Box::new(Bound::Literal(10)), Operation::Subtract, Box::new(Bound::Literal(5)));
            let result = Bound::calculate_bound(&test_calculation, &ctx);
            assert_eq!(result.unwrap(), 5);
        }

        #[test]
        fn test_bound_complex() {
            let ctx: HashMap<String, i32> = HashMap::new();
            let sub_calculation: Bound = Bound::Calculation(Box::new(Bound::Literal(10)), Operation::Subtract, Box::new(Bound::Literal(8)));
            let add_calculation: Bound = Bound::Calculation(Box::new(Bound::Literal(3)), Operation::Add, Box::new(sub_calculation));
            let mult_calculation: Bound = Bound::Calculation(Box::new(Bound::Literal(5)), Operation::Multiply, Box::new(add_calculation));
            let result = Bound::calculate_bound(&mult_calculation, &ctx);
            assert_eq!(result.unwrap(), 25);
        }

        #[test]
        fn test_bound_str_creation() {
            let mut parser = ExpressionParser::new();
            let bound_str = "5*4-10*2".into();
            let bound = parser.get_bound_from_string(&bound_str, 0).unwrap();
            let result = Bound::calculate_bound(&bound, &parser.context);
            assert_eq!(result.unwrap(), 0);
        }

        #[test]
        fn test_bound_str_creation_var() {
            let mut parser = ExpressionParser::new();
            let bound_str = "5*a-10".into();
            let bound = parser.get_bound_from_string(&bound_str, 0).unwrap();
            parser.context.insert("a".into(), 4);
            let result = Bound::calculate_bound(&bound, &parser.context);
            assert_eq!(result.unwrap(), 10);
        }

        #[test]
        fn test_bound_str_creation_vars() {
            let mut parser = ExpressionParser::new();
            let bound_str = "a*b-c+a".into();
            let bound = parser.get_bound_from_string(&bound_str, 0).unwrap();
            parser.context.insert("a".to_string(), 5);
            parser.context.insert("b".to_string(), 4);
            parser.context.insert("c".to_string(), 10);
            let result = Bound::calculate_bound(&bound, &parser.context);
            assert_eq!(result.unwrap(), 15);
        }

        #[test]
        fn test_bound_complex_bracket() {
            let mut parser = ExpressionParser::new();
            let bound_str = "(2-1)*3+(3*(7-4)-1)-2".into();
            let bound = parser.get_bound_from_string(&bound_str, 0).unwrap();
            let result = Bound::calculate_bound(&bound, &parser.context);
            assert_eq!(result.unwrap(), 9);
        }

        #[test]
        fn test_bound_implicit_mult() {
            let mut parser = ExpressionParser::new();
            let bound_str = "5(6)+2x+2(3)(4)-3(2(2))".into();
            let bound = parser.get_bound_from_string(&bound_str, 0).unwrap();
            parser.context.insert("x".into(), 5);
            let result = Bound::calculate_bound(&bound, &parser.context);
            assert_eq!(result.unwrap(), 52);
        }

        #[test]
        fn test_bound_invalid_op() {
            let mut parser = ExpressionParser::new();
            let bound_str = "5+(+5)".into();
            assert!(parser.get_bound_from_string(&bound_str, 0).is_err());
        }

        #[test]
        fn test_bound_invalid_bracket() {
            let mut parser = ExpressionParser::new();
            let bound_str = ")5+4(".into();
            assert!(parser.get_bound_from_string(&bound_str, 0).is_err());
        }
    }

    mod repetition_parser {
        use super::*;

        fn test_repetition(result: Token, a_result: i32, b_result: i32) {
            match result {
                Token::Repetition(_, Bound::Literal(a), Bound::Literal(b)) => {
                    assert!(a == a_result && b == b_result);
                }
                _ => panic!()          
            }
        }

        #[test]
        fn test_preset_repetition_parsing() {
            let mut parser = ExpressionParser::new();
            let mut result = parser.parse_repetition(Token::Sequence(vec![Token::Literal("a".into())]), &vec!['*'], 0).unwrap().0;
            test_repetition(result, 0, REPEAT_LIMIT);

            result = parser.parse_repetition(Token::Sequence(vec![Token::Literal("a".into())]), &vec!['+'], 0).unwrap().0;
            test_repetition(result, 1, REPEAT_LIMIT);

            result = parser.parse_repetition(Token::Sequence(vec![Token::Literal("a".into())]), &vec!['?'], 0).unwrap().0;
            test_repetition(result, 0, 1);
        }

        #[test]
        fn test_custom_repetition_parsing() {
            let mut parser = ExpressionParser::new();
            let mut result = parser.parse_repetition(Token::Sequence(vec![Token::Literal("a".into())]), &"{1,3}".chars().collect(), 0).unwrap().0;
            test_repetition(result, 1, 3);

            result = parser.parse_repetition(Token::Sequence(vec![Token::Literal("a".into())]), &"{,3}".chars().collect(), 0).unwrap().0;
            test_repetition(result, 0, 3);

            result = parser.parse_repetition(Token::Sequence(vec![Token::Literal("a".into())]), &"{1,}".chars().collect(), 0).unwrap().0;
            test_repetition(result, 1, REPEAT_LIMIT);

            result = parser.parse_repetition(Token::Sequence(vec![Token::Literal("a".into())]), &"{,}".chars().collect(), 0).unwrap().0;
            test_repetition(result, 0, REPEAT_LIMIT);

            result = parser.parse_repetition(Token::Sequence(vec![Token::Literal("a".into())]), &(format!("{{{},{}}}", REPEAT_LIMIT, REPEAT_LIMIT)).chars().collect(), 0).unwrap().0;
            test_repetition(result, REPEAT_LIMIT, REPEAT_LIMIT);
        }

        #[test]
        fn test_complex_repetition_parsing() {
            let mut parser = ExpressionParser::new();
            let mut result = parser.parse_repetition(Token::Sequence(vec![Token::Literal("a".into())]), &"{(2+a)b,(b)(((b)}".chars().collect(), 0).unwrap().0;
            parser.context.insert("a".into(), 2);
            parser.context.insert("b".into(), 5);
            match result {
                Token::Repetition(_, bound_a, bound_b) => {
                    let a = Bound::calculate_bound(&bound_a, &parser.context).unwrap();
                    let b = Bound::calculate_bound(&bound_b, &parser.context).unwrap();
                    assert!(a == 20 && b == 25);
                }
                _ => panic!()
            }
            result = parser.parse_repetition(Token::Sequence(vec![Token::Literal("a".into())]), &"{01+2+3+4,(1+2)-3*02+4*(4}".chars().collect(), 0).unwrap().0;
            match result {
                Token::Repetition(_, bound_a, bound_b) => {
                    let a = Bound::calculate_bound(&bound_a, &parser.context).unwrap();
                    let b = Bound::calculate_bound(&bound_b, &parser.context).unwrap();
                    assert!(a == 10 && b == 13);
                }
                _ => panic!()
            }          
        }
    
        #[test]
        // the ranges themselves are not able to be directly checked at this point due to the effect of variables, so syntax is the only place where errors can be caught
        fn test_invalid_reptition_parsing() {
            let mut parser = ExpressionParser::new();
            assert!(parser.parse_repetition(Token::Sequence(vec![Token::Literal("a".into())]), &"{2,&2}".chars().collect(), 0).is_err());
            assert!(parser.parse_repetition(Token::Sequence(vec![Token::Literal("a".into())]), &"{2,(4))+1}".chars().collect(), 0).is_err());
            assert!(parser.parse_repetition(Token::Sequence(vec![Token::Literal("a".into())]), &"{2,4".chars().collect(), 0).is_err());
            assert!(parser.parse_repetition(Token::Sequence(vec![Token::Literal("a".into())]), &"{2,4(}".chars().collect(), 0).is_err());
        }
    }

    mod general_parser {
        use super::*;

        fn test_expression(expression: &str, expected_token: Token) {
            let output_token = ExpressionParser::produce_token(expression.into()).unwrap().token;
            assert!(output_token == expected_token, "\nExpected:\n{}\nGot:\n{}", ExpressionParser::get_string(&expected_token), ExpressionParser::get_string(&output_token));
        }

        fn test_error(expression: &str) {
            let output_token = ExpressionParser::produce_token(expression.into());
            assert!(output_token.is_err(), "Invalid expression '{}' incorrectly parsed as valid\nExpression tree:\n{}", expression, ExpressionParser::get_string(&output_token.unwrap().token));
        }

        #[test]
        fn parse_choice() {
            let expected_token = Token::Choice(vec![
                Token::Literal("abc".into()), 
                Token::Literal("def".into()), 
                Token::Literal("ghi".into())
            ]);
            test_expression("abc|def|ghi", expected_token);
        }

        #[test]
        fn parse_choice_sequence() {
            let expected_token = Token::Sequence(vec![
                Token::Literal("a".into()),
                Token::Choice(vec![
                    Token::Literal("ab".into()),
                    Token::Literal("cd".into()),
                ]),
                Token::Choice(vec![
                    Token::Literal("a".into()),
                    Token::Literal("b".into()),
                ]),
                Token::Literal("b".into())
            ]);
            test_expression("a(ab|cd)(a|b)b", expected_token);
        }

        #[test]
        fn parse_repetition() {
            let mut expected_token = Token::Repetition(
                Box::new(Token::Literal("a".into())),
                Bound::Literal(1), 
                Bound::Literal(5)
            );
            test_expression("a{1,5}", expected_token);

            expected_token = Token::Repetition(
                Box::new(Token::Literal("a".into())),
                Bound::Literal(1), 
                Bound::Literal(REPEAT_LIMIT)
            );
            test_expression("a{1,}", expected_token);

            expected_token = Token::Repetition(
                Box::new(Token::Literal("a".into())),
                Bound::Literal(0), 
                Bound::Literal(REPEAT_LIMIT)
            );
            test_expression("a{,}", expected_token);
        }

        #[test]
        fn parse_multi_repetition() {
            let expected_token = Token::Sequence(vec![
                Token::Literal("ab".into()),
                Token::Repetition(Box::new(Token::Literal("a".into())), Bound::Literal(0), Bound::Literal(1)),
                Token::Repetition(Box::new(Token::Literal("b".into())), Bound::Literal(1), Bound::Literal(REPEAT_LIMIT)),
                Token::Repetition(Box::new(Token::Literal("c".into())), Bound::Literal(1), Bound::Literal(3)),
                Token::Literal("cd".into()),
            ]);
            test_expression("ab(a?)(b+)(c{1,3})cd", expected_token);
        }

        #[test]
        fn parse_excessive_bracketing() {
            let expected_token = Token::Repetition(
                Box::new(Token::Literal("a".into())),
                Bound::Literal(0), 
                Bound::Literal(1)
            );
            test_expression("(((((a))?)))", expected_token);
        }

        #[test]
        fn parse_nested_repetition() {
            let mut expected_token = Token::Sequence(vec![
                Token::Literal("ab".into()),
                Token::Repetition(Box::new(
                    Token::Sequence(vec![
                        Token::Literal("a".into()),
                        Token::Repetition(
                            Box::new(Token::Literal("c".into())), Bound::Literal(1), Bound::Literal(3)
                        ),
                        Token::Literal("a".into())])),
                Bound::Literal(0), Bound::Literal(1))
            ]);
            test_expression("ab((a(c{1,3})a)?)", expected_token);

            expected_token = Token::Repetition(
                Box::new(Token::Repetition(
                    Box::new(Token::Repetition(
                        Box::new(Token::Repetition(
                            Box::new(Token::Repetition(Box::new(Token::Literal("a".into())), Bound::Literal(0), Bound::Literal(1))
                        ), Bound::Literal(0), Bound::Literal(1))
                    ), Bound::Literal(0), Bound::Literal(REPEAT_LIMIT))
                ), Bound::Literal(1), Bound::Literal(REPEAT_LIMIT))
            ), Bound::Literal(0), Bound::Literal(1));
            test_expression("a??*+?", expected_token);
        }

        #[test]
        fn parse_nested_complex() {
            let expected_token = Token::Choice(vec![
                Token::Sequence(vec![
                    Token::Literal("a".into()),
                    Token::Repetition(
                        Box::new(Token::Choice(vec![
                            Token::Sequence(vec![
                                Token::Repetition(
                                    Box::new(Token::Literal("a".into())),
                                    Bound::Literal(1),
                                    Bound::Literal(2)
                                ),
                                Token::Literal("bc".into())
                            ]),
                            Token::Repetition(
                                Box::new(Token::Literal("e".into())),
                                Bound::Literal(0),
                                Bound::Literal(1)
                            ),
                            Token::Sequence(vec![
                                Token::Literal("gg".into()),
                                Token::Choice(vec![
                                    Token::Literal("a".into()),
                                    Token::Literal("b".into()),
                                    Token::Choice(vec![
                                        Token::Repetition(
                                            Box::new(Token::Literal("c".into())),
                                            Bound::Literal(3),
                                            Bound::Literal(3)
                                        ),
                                        Token::Repetition(
                                            Box::new(Token::Literal("d".into())),
                                            Bound::Literal(1),
                                            Bound::Literal(3)
                                        )
                                    ])
                                ])
                            ])
                        ])),
                        Bound::Literal(1),
                        Bound::Literal(2)
                    )
                ]),
                Token::Literal("abc".into())
            ]);
            test_expression("a(a{1,2}bc|e?|gg(a|b|(c{3}|d{1,3}))){1,2}|abc".into(), expected_token);
        }

        #[test]
        fn parse_unclean() {
            let expected_token = Token::Sequence(vec![
                Token::Literal("a".into()),
                Token::Choice(vec![
                    Token::Literal("ab".into()),
                    Token::Literal("cd".into()),
                ]),
                Token::Choice(vec![
                    Token::Literal("a".into()),
                    Token::Literal("b".into()),
                ]),
                Token::Literal("b".into())
            ]);
            test_expression("   \na\t(ab| c d  ) \n(a | b)\n\tb", expected_token);            
        }
    
        #[test]
        fn parse_invalid() {
            test_error("a{}.");
            test_error("((a)()");
            test_error("|a|b");
            test_error("(a{)a,b}");
            test_error("?a");
            test_error("a({a, b})");
        }
    }
}