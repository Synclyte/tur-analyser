use plotters::style::full_palette::GREY_800;
use wasm_bindgen::prelude::wasm_bindgen;
use yew::prelude::*;
use plotters::prelude::*;
use plotters_canvas::CanvasBackend;
use web_sys::HtmlCanvasElement;
use tur::Program;
use tur::expression::analyse_expression;
use tur::expression::{AnalysisInfo, Complexity};
use web_sys::HtmlTextAreaElement;

#[derive(Properties, PartialEq)]
pub struct AnalyserProps {
    pub program: Program,
}

#[derive(Properties, PartialEq)]
pub struct ChartProps {
    pub data: Vec<(usize, usize)>,
}

#[wasm_bindgen(module = "/runtime-chart.js")]
extern "C" {
    fn draw_runtime_chart(canvas_id: &str, x_data: Vec<f64>, y_data: Vec<f64>);
}

#[function_component(RuntimeChart)]
fn runtime_chart(props: &ChartProps) -> Html {
    let data = props.data.clone();

    use_effect_with(data.clone(), move |points| {
        if !points.is_empty() {
            let min_x = points.first().map(|(x, _)| *x).unwrap_or(0);
            let max_x = points.last().map(|(x, _)| *x).unwrap_or(0);

            let mut x_data: Vec<f64> = Vec::new();
            let mut y_data: Vec<f64> = Vec::new();

            let mut current_x = 0;
            // adds padding for non-continuous points
            for x in min_x..=max_x {
                let current_data_point = points[current_x];
                x_data.push(x as f64);
                // if a value for the current x exists in the graph data, push it
                // otherwise, push NAN to pad
                if current_x < points.len() && current_data_point.0 == x {
                    y_data.push(current_data_point.1 as f64);
                    current_x += 1;
                } else {
                    y_data.push(f64::NAN);
                }
            }
            
            draw_runtime_chart("complexity-chart", x_data, y_data);
        }
        || ()
    });

    html! {
        <div class="w-full h-80 relative bg-base-100 rounded-box">
            <canvas id="complexity-chart"></canvas>
        </div>
    }
}

#[function_component(ComplexityAnalyser)]
pub fn complexity_analyser(props: &AnalyserProps) -> Html {
    let regex_input = use_state(|| "".to_string());
    let is_analysing = use_state(|| false);
    let analysis_result = use_state(|| None::<Result<AnalysisInfo, String>>);

    let on_input_change = {
        let regex_input = regex_input.clone();
        Callback::from(move |e: InputEvent| {
            let target = e.target_unchecked_into::<web_sys::HtmlTextAreaElement>();
            
            let _ = target.set_attribute("style", "height: auto; overflow: hidden;");
            let scroll_height = target.scroll_height();
            let _ = target.set_attribute("style", &format!("height: {}px; overflow: hidden;", scroll_height));
            
            regex_input.set(target.value());
        })
    };

    let on_analyse_click = {
        let regex_input = regex_input.clone();
        let is_analysing = is_analysing.clone();
        let analysis_result = analysis_result.clone();
        let program = props.program.clone();

        Callback::from(move |_| {
            if regex_input.is_empty() { return; }

            is_analysing.set(true);
            analysis_result.set(None);

            let async_regex = regex_input.clone();
            let async_analysing = is_analysing.clone();
            let async_result = analysis_result.clone();
            let async_program = program.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let _ = gloo_timers::future::sleep(std::time::Duration::from_millis(15)).await;
                let result = analyse_expression(&async_regex, &async_program);
                
                async_result.set(Some(result));
                async_analysing.set(false);
            });
        })
    };

    let modal_opened = use_state(|| false);
    let open_modal = {
        let modal_opened = modal_opened.clone();
        Callback::from(move |_e: MouseEvent| modal_opened.set(true))
    };

    let close_modal = {
        let modal_opened = modal_opened.clone();
        Callback::from(move |_e: MouseEvent| modal_opened.set(false))
    };

    let on_modal_keydown = {
        let modal_opened = modal_opened.clone();
        Callback::from(move |e: KeyboardEvent| {
            if e.key() == "Escape" {
                modal_opened.set(false);
            }
        })
    };



    html! {
        <div class="card card-compact bg-base-100 mt-4 shadow-md border border-base-200">
            <div class="card-body">
                // styling with tailwind classes appears to be heavily broken here. help button alignment was ignored
                // this forces base css to apply the styles instead
                <style>
                    {".analyser-header-flex { display: flex; align-items: center; justify-content: space-between; width: 100%; border-bottom: 1px solid rgba(255, 255, 255, 1); padding-bottom: 0.5rem; margin-bottom: 0.5rem; }"}
                    {".analyser-header-flex > button { width: 30px; height: 30px; border-radius: 50%; background-color: rgba(128, 128, 128, 0.3); color: white; border: none; display: flex; align-items: center; justify-content: center; font-size: 18px; cursor: pointer; transition: opacity 0.2s ease; padding: 0; margin: 0; }"}
                    {".analyser-header-flex > button:hover { opacity: 0.5; }"}
                </style>
                
                <div class="analyser-header-flex">
                    <h3 class="card-title text-lg" style="margin: 0;">
                        {"Complexity Analysis"}
                    </h3>
                    <button
                        onclick={open_modal}
                        title="Help"
                    >
                        {"?"}
                    </button>
                </div>
                
                <textarea 
                    class="textarea textarea-bordered w-full font-mono text-sm shadow-inner resize-none leading-relaxed"
                    rows="2"
                    value={(*regex_input).clone()}
                    oninput={on_input_change}
                    disabled={*is_analysing}
                    placeholder="Enter Regex input generation expression..."
                    style="overflow: hidden;"
                />
                <div style="display: flex; justify-content: flex-end; width: 100%; margin-top: 0.75rem;">
                    <button 
                        class="btn btn-primary px-8"
                        onclick={on_analyse_click} 
                        disabled={*is_analysing || regex_input.is_empty()}
                    >
                        { if *is_analysing { "Simulating Machine" } else { "Run Analysis" } }
                    </button>
                </div>

                {
                    match &*analysis_result {
                        None => html! {},
                        Some(Err(e)) => html! {
                            <div 
                                class="alert alert-error shadow-sm mt-4"
                                style="padding-top: 30px;"
                            >
                                <span>{ format!("Error: {}", e) }</span>
                            </div>
                        },
                        Some(Ok(info)) => {
                            let mut sorted_states: Vec<(&String, &Complexity)> = info.estimated_state_complexities
                                .iter()
                                .filter(|(state, _)| *state != "start" && *state != "stop")
                                .collect();

                            sorted_states.sort_by(|a, b| a.0.cmp(b.0));

                            html! {
                                <div 
                                    class="grid grid-cols-1 lg:grid-cols-3 gap-8 mt-6 pt-6 border-t border-base-200"
                                >
                                    <div class="col-span-1 lg:col-span-2">
                                        <RuntimeChart data={info.graph_data.clone()} />
                                    </div>
                                    
                                    <div class="col-span-1 flex flex-col gap-6 h-full">
                                        
                                        <div class="bg-base-200 rounded-box p-6 flex flex-col justify-center items-center border border-base-300 shadow-sm" style="padding-bottom:20px;">
                                            <h3 class="card-title text-lg text-white mb-3">{"Overall Complexity"}</h3>
                                            <span class="text-4xl text-primary">{ format!("{}", info.estimated_complexity) }</span>
                                        </div>
                                        
                                        <div class="flex-grow overflow-hidden rounded-box border border-base-300 bg-base-100 shadow-sm flex flex-col">
                                            
                                            <div class="bg-base-200 p-4 border-b border-base-300">
                                                <h3 class="card-title text-lg text-white mb-3">{"State Breakdown"}</h3>
                                            </div>
                                            
                                            <div class="overflow-y-auto" style="max-height: 350px;">
                                                <div class="border border-base-300 rounded-b-box justify-center" style="display: flex; flex-direction: column; width: 100%;">
                                                    
                                                    <div class="bg-base-300 text-white border-b border-base-300" style="display: flex; width: 100%;">
                                                        <div class="border-r border-base-300 font-bold text-base" style="min-width: 100px; padding: 0.1rem; text-align: left;">
                                                            {"State Name"}
                                                        </div>
                                                        <div class="font-bold text-base" style="min-width: 100px; padding: 0.1rem; text-align: left;">
                                                            {"Time Complexity"}
                                                        </div>
                                                    </div>
                                                    
                                                    // THE ROWS
                                                    <div style="display: flex; flex-direction: column; width: 100%;">
                                                        { for sorted_states.iter().map(|(state, comp)| html! {
                                                            <div class="border-b border-base-300 hover:bg-base-200 transition-colors" style="display: flex; width: 100%;">
                                                                <div class="border-r border-base-300 font-mono text-base" style="min-width: 100px; padding: 0.1rem; display: flex; align-items: center; justify-content: flex-start;">
                                                                    { state }
                                                                </div>
                                                                <div class="text-primary font-bold text-base" style="min-width: 100px; padding: 0.1rem; display: flex; align-items: center; justify-content: flex-start;">
                                                                    { format!("{}", comp) }
                                                                </div>
                                                            </div>
                                                        }) }
                                                    </div>
                                                    
                                                </div>
                                            </div>
                                            
                                        </div>
                                    </div>
                                </div>
                            }
                        }
                    }
                }

                <dialog
                    id="analysis_help_modal"
                    class={classes!("modal", if *modal_opened { Some("modal-open") } else { None })}
                    onkeydown={on_modal_keydown}
                >
                    <div class="modal-box w-11/12 max-w-3xl relative" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                        <button
                            class="btn btn-sm btn-circle btn-ghost"
                            onclick={close_modal.clone()}
                            style="position: absolute; top: 12px; right: 12px;"
                        >
                            {"×"}
                        </button>
                        
                        <h3 class="font-bold text-lg">{"Complexity Analysis Help"}</h3>
                        <div class="help-content">
                            <h4 class="help-heading">{"The Premise:"}</h4>
                            <p class="help-paragraph">{"
                            Time Complexity analysis of a Turing Machine is done by providing it with inputs and measuring the number of steps taken for that input to terminate. 
                            This can require a large number of inputs to do correctly, and many simple Turing Machines are highly formulaic, making this a waste of time.
                            Therefore, a variant of regular expressions can be used here to automatically generate expressions.
                            "}</p>

                            <h4 class="help-heading">{"Regular Expressions:"}</h4>
                            <p class="help-paragraph">{"
                            The variant of regular expressions used here has a simplified syntax, and supports different features compared to typical RegEx. For example:
                            "}</p>
                            <ul class="help-list">
                                <li class="help-list-item">{"Generate either '0', '1', or '22': "}<code>{"(0|1|22)"}</code></li>
                                <li class="help-list-item">{"Repeat '0' 10-20 times: "}<code>{"0{10,20}"}</code></li>
                                <li class="help-list-item">{"A simple generator for binary addition inputs: "} <code>{"$(0|1)*"}</code></li>
                                <li class="help-list-item">{"A simple even number generator: "}<code>{"(1|0){2}+0"}</code></li>
                            </ul>

                            <h4 class="help-heading">{"Advanced Features:"}</h4>
                            <p class="help-paragraph">{"Format: "} <code>{"current_symbol -> new_symbol, direction, next_state"}</code></p>
                            <ul class="help-list">
                                <li class="help-list-item">{"Directions: "} <code>{"L"}</code> {" (left), "} <code>{"R"}</code> {" (right), "} <code>{"S"}</code> {" (stay)"}</li>
                                <li class="help-list-item">{"Use "} <code>{"_"}</code> {" as a special symbol to match/write the program's blank symbol (e.g., if blank is ' ', then '_' matches ' ')"}</li>
                            </ul>
                        </div>
                    </div>
                    
                    // Backdrop click closes the modal
                    <form method="dialog" class="modal-backdrop" onclick={close_modal.clone()}>
                        <button>{"close"}</button>
                    </form>
                </dialog>
            </div>
        </div>
    }
}