use crate::components::MachineState;
use yew::{function_component, html, Callback, Event, Html, Properties, TargetCast, InputEvent};

#[derive(Properties, PartialEq)]
pub struct TapeViewProps {
    pub tapes: Vec<Vec<char>>,
    pub head_positions: Vec<usize>,
    pub auto_play: bool,
    pub machine_state: MachineState,
    pub is_program_ready: bool,
    pub blank_symbol: char,
    pub state: String,
    pub step_count: usize,
    pub current_symbols: Vec<char>,
    pub on_step: Callback<()>,
    pub on_reset: Callback<()>,
    pub on_toggle_auto: Callback<()>,
    pub speed: u64,
    pub on_speed_change: Callback<u64>,
    pub tape_left_offsets: Vec<usize>,
    pub message: String,
    pub on_toggle_analysis: Callback<()>,
    pub show_analysis: bool,
}

#[function_component(TapeView)]
pub fn tape_view(props: &TapeViewProps) -> Html {
    let cell_width = 42; // Width of each cell (no gap)
    let padding_cells = 15; // Number of blank cells to show on each side for infinite tape effect

    let is_machine_running = props.machine_state == MachineState::Running;
    let current_speed = (500f64 / props.speed as f64).log2().round() as i32;
    let on_speed_input = {
        let on_speed_change = props.on_speed_change.clone();
        Callback::from(move |e: InputEvent| {
            let input = e.target_unchecked_into::<web_sys::HtmlInputElement>();
            if let Ok(v) = input.value().parse::<i32>() {
                let multiplier = 2f64.powi(v);
                let delay = (500f64 / multiplier).max(1.0) as u64;
                on_speed_change.emit(delay);
            }
        })
    };

    html! {
        <div class="tape-view">
            <div class="tape-header">
                <h3 class="card-title">{"Tapes"}</h3>
                    <div class="tape-controls flex flex-wrap items-center gap-2">
                        <button
                            class="btn btn-primary"
                            onclick={props.on_step.reform(|_| ())}
                            disabled={!is_machine_running || !props.is_program_ready}
                        >
                            {"Step"}
                        </button>
                        <button
                            class="btn btn-secondary"
                            onclick={props.on_reset.reform(|_| ())}
                            disabled={!props.is_program_ready}
                        >
                            {"Reset"}
                        </button>
                        <button
                            class={format!("btn {}", if props.auto_play { "btn-warning" } else { "btn-success" })}
                            onclick={props.on_toggle_auto.reform(|_| ())}
                            disabled={!is_machine_running || !props.is_program_ready}
                        >
                            {if props.auto_play { "Pause" } else { "Auto" }}
                        </button>

                        <div class="flex items-center gap-2 px-3 py-2 bg-base-200 rounded-btn border border-base-300">
                            <label class="text-sm font-medium text-base-content">{"Speed:"}</label>
                            <input
                                type="range"
                                min="-2"
                                max="8"
                                step="1"
                                value={current_speed.to_string()}
                                class="range range-sm range-primary w-24 sm:w-32"
                                oninput={on_speed_input}
                            />
                            <span class="text-sm font-mono w-14 text-right text-base-content tabular-nums inline-block">
                                { format!("{}x", 2f64.powi(current_speed)) }
                            </span>
                        </div>

                        <button 
                            class={format!("btn {}", if props.show_analysis { "btn-active btn-secondary" } else { "btn-outline" })}
                            onclick={props.on_toggle_analysis.reform(|_| ())}
                            disabled={!props.is_program_ready}
                        >
                            { "Analyser" }
                        </button>
                    </div>
            </div>
            <div class="tapes-container">
                {props.tapes.iter().enumerate().map(|(tape_index, tape)| {
                    let head_position = props.head_positions.get(tape_index).cloned().unwrap_or(0);
                    let left_offset = *props.tape_left_offsets.get(tape_index).unwrap_or(&0);

                    // Add padding cells on the left
                    let mut visible_tape = vec![props.blank_symbol; padding_cells - left_offset];

                    // Add the actual tape content
                    for &symbol in tape {
                        visible_tape.push(symbol);
                    }

                    // Add padding cells on the right
                    visible_tape.extend(std::iter::repeat_n(props.blank_symbol, padding_cells));

                    // Calculate the transform to center the active cell under the head pointer
                    let active_cell_index = padding_cells + head_position - left_offset;
                    let cell_offset = active_cell_index as i32 * cell_width;

                    // js_sys::eval(&format!("console.log({active_cell_index}, {cell_offset}, {left_offset})")).unwrap();
                    let transform_style = format!(
                        "transform: translateX(calc(50% - {cell_offset}px - {}px)) translateY(-50%)",
                        cell_width / 2
                    );

                    html! {
                        <div key={tape_index} class="single-tape">
                            {html!{
                                <div class="tape-label">
                                    {format!("#{} ", tape_index + 1)}
                                    <span class="head-info-inline">
                                        {format!("(head: {head_position})")}
                                    </span>
                                </div>
                            }}
                            <div class="tape-machine">
                                <div class="tape-container" style={transform_style}>
                                    {visible_tape.iter().enumerate().map(|(i, &symbol)| {
                                        // Check if this cell is under the head
                                        let is_under_head = i == active_cell_index;

                                        let class = if is_under_head {
                                            "tape-cell under-head"
                                        } else {
                                            "tape-cell"
                                        };

                                        html! {
                                            <div key={format!("{tape_index}_{i}_{symbol}_{i}")} class={class}>
                                                {symbol}
                                            </div>
                                        }
                                    }).collect::<Html>()}
                                </div>
                                <div class="tape-head">
                                    <div class="head-pointer"></div>
                                </div>
                            </div>
                        </div>
                    }
                }).collect::<Html>()}
            </div>

            <div class="state-info">
                <div class="state-item">
                    <span class="label">{"State"}</span>
                    <span class="value state-name">{&props.state}</span>
                </div>
                <div class="state-item">
                    <span class="label">{"Steps"}</span>
                    <span class="value">{props.step_count}</span>
                </div>
                <div class="state-item">
                    <span class="label">{"Symbols"}</span>
                    <span class="value">
                        {props.current_symbols.iter().map(|&symbol| {
                            html! { <span class="symbol">{symbol}</span> }
                        }).collect::<Html>()}
                    </span>
                </div>
                <div class="state-item">
                    <span class="label">{"Status"}</span>
                    <span class={match props.machine_state {
                        MachineState::Halted => "value status halted",
                        MachineState::Running if props.auto_play => "value status running",
                        MachineState::Running => "value status ready",
                    }}>
                        {match props.machine_state {
                            MachineState::Halted => "HALTED",
                            MachineState::Running if props.auto_play => "RUNNING",
                            MachineState::Running => "READY",
                        }}
                    </span>
                </div>
            </div>
            {if !props.message.is_empty() {
                html! { <div class="status-message">{&props.message}</div> }
            } else {
                html! {}
            }}
        </div>
    }
}
