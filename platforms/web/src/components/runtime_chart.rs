// on clicking a magnifying glass button:
//     open a textbox for inserting tape inputs
//     allow many inputs, delimited by a newline or a semicolon
//
// when the user clicks the "analyse" button at the bottom of this textbox:
//     feed all provided inputs into a turing machine, taking the runtime of each input. if runtime exceeds a limit, return a special output to indicate non-termination
//     graph these inputs by bars, the width of each bar determined by the total number of inputs. generate appropriate x and y axis values:
//          the height of the bar should be determined by the average runtime of the input tapes of the corresponding length
//     provide a runtime estimate based on the slope of the bar chart
//
// extra:
// provide another button to automatically generate inputs for the bar chart:
//     inputs should be generated using a multi-generation genetic algorithm running for input lengths 1-8 by default
//     could fall back to random generation if alphabet is too big for the genetic algorithm to successfully converge

use crate::components::ProgramSelector;
use tur::{parser::parse, Program, TuringMachineError, MAX_PROGRAM_SIZE};
use web_sys::HtmlTextAreaElement;
use yew::prelude::*;

#[derive(Properties)]
pub struct RuntimeChartProps {

}