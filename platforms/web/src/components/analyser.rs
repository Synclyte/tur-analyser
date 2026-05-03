use wasm_bindgen::prelude::wasm_bindgen;
use yew::prelude::*;
use tur::Program;
use tur::expression::analyse_expression;
use tur::expression::{AnalysisInfo, Complexity};
use std::collections::HashMap;

#[derive(Properties, PartialEq)]
pub struct AnalyserProps {
    pub program: Program,
}

#[derive(Properties, PartialEq)]
pub struct ChartProps {
    pub id: String,
    pub data: Vec<(usize, usize)>,
    #[prop_or_default]
    pub is_small: bool,
}

#[wasm_bindgen(module = "/runtime-chart.js")]
extern "C" {
    fn draw_runtime_chart(canvas_id: &str, x_data: Vec<f64>, y_data: Vec<f64>, is_small: bool);
}

#[function_component(RuntimeChart)]
fn runtime_chart(props: &ChartProps) -> Html {
    let data = props.data.clone();
    let id = props.id.clone();
    let is_small = props.is_small.clone();

    let height_style = if is_small { "120px" } else { "320px" };

    use_effect_with((data.clone(), id.clone(), is_small.clone()), move |(points, canvas_id, is_small)| {
        if !points.is_empty() {
            let min_x = points.first().map(|(x, _)| *x).unwrap_or(0);
            let max_x = points.last().map(|(x, _)| *x).unwrap_or(0);

            let mut x_data: Vec<f64> = Vec::new();
            let mut y_data: Vec<f64> = Vec::new();

            let mut points_map = HashMap::new();
            for &(x, y) in points.iter() {
                points_map.entry(x)
                    .and_modify(|current_y| *current_y = y.max(*current_y))
                    .or_insert(y);
            }

            for x in min_x..=max_x {
                x_data.push(x as f64);
                if let Some(&y) = points_map.get(&x) {
                    y_data.push(y as f64);
                } else {
                    y_data.push(f64::NAN);
                }
            }
            
            draw_runtime_chart(canvas_id, x_data, y_data, *is_small);
        }
        || ()
    });

    html! {
        <div style={format!("width: 100%; position: relative; overflow: hidden; height: {};", height_style)}>
            <canvas id={props.id.clone()} style="position: absolute; top: 0; left: 0; width: 100%; height: 100%; display: block;"></canvas>
        </div>
    }
}

#[function_component(ComplexityAnalyser)]
pub fn complexity_analyser(props: &AnalyserProps) -> Html {
    // ignores termination-like state names in analysis, as these are only ever visited once - pointless to build a graph to show the user these
    let ignored_state_names: Vec<String> = vec!["stop".to_string(), "accept".to_string(), "reject".to_string()];

    // holds regex string to be processed
    let regex_input = use_state(|| "".to_string());
    // encodes whether analysis is currently happening
    let is_analysing = use_state(|| false);
    // if true, only adds a generated string to the end graph if it terminates in a state containing "accept" in its name
    let is_strict = use_state(|| false);
    // number of attempts to make to generate strings for each length. always uses the string with the longest length
    let generation_attempts = use_state(|| 1);
    // data passed back from analysis
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

    let on_strict_change = {
        let is_strict = is_strict.clone();
        Callback::from(move |e: Event| {
            let target = e.target_unchecked_into::<web_sys::HtmlInputElement>();
            is_strict.set(target.checked());
        })
    };

    let on_attempts_change = {
        let generation_attempts = generation_attempts.clone();
        Callback::from(move |e: Event| {
            let target = e.target_unchecked_into::<web_sys::HtmlSelectElement>();
            if let Ok(val) = target.value().parse::<usize>() {
                generation_attempts.set(val);
            }
        })
    };

    let on_analyse_click = {
        let regex_input = regex_input.clone();
        let is_analysing = is_analysing.clone();
        let analysis_result = analysis_result.clone();
        let program = props.program.clone();
        let is_strict = is_strict.clone();
        let generation_attempts = generation_attempts.clone();

        Callback::from(move |_| {
            if regex_input.is_empty() || !program.is_single_tape() { return; }

            is_analysing.set(true);
            analysis_result.set(None);

            let async_regex = regex_input.clone();
            let async_analysing = is_analysing.clone();
            let async_result = analysis_result.clone();
            let async_program = program.clone();

            let async_strict = *is_strict;
            let async_attempts = *generation_attempts;

            wasm_bindgen_futures::spawn_local(async move {
                let _ = gloo_timers::future::sleep(std::time::Duration::from_millis(15)).await;
                let result = analyse_expression(&async_regex, &async_program, async_strict, async_attempts);
                
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

                    {".modal-close-btn { position: absolute; top: 12px; right: 12px; background: transparent; border: none; color: inherit; font-size: 24px; width: 32px; height: 32px; border-radius: 50%; cursor: pointer; display: flex; align-items: center; justify-content: center; transition: all 0.2s ease; opacity: 0.7; padding: 0; }"}
                    {".modal-close-btn:hover { background-color: rgba(128, 128, 128, 0.2); opacity: 1; }"}
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
                <div style="display: flex; justify-content: flex-end; width: 100%; margin-top: 0.75rem; gap: 10px;">
                    <label class="cursor-pointer" style="display: flex; align-items: center; gap: 0.5rem;">
                        <span class="text-sm">{"Analyse Accepted Only"}</span>
                        <input 
                            type="checkbox" 
                            class="checkbox checkbox-sm checkbox-primary" 
                            checked={(*is_strict).clone()} 
                            onchange={on_strict_change} 
                            disabled={*is_analysing} 
                        />
                    </label>
                        
                    <div style="display: flex; align-items: center; gap: 0.5rem;">
                        <span class="text-sm">{"Attempts:"}</span>
                        <select 
                            class="select select-bordered select-sm" 
                            onchange={on_attempts_change} 
                            disabled={*is_analysing} 
                            value={(*generation_attempts).clone().to_string()}
                        >
                            <option value="1" selected={*generation_attempts == 1}>{"1"}</option>
                            <option value="2" selected={*generation_attempts == 2}>{"2"}</option>
                            <option value="5" selected={*generation_attempts == 5}>{"5"}</option>
                            <option value="10" selected={*generation_attempts == 10}>{"10"}</option>
                        </select>
                    </div>

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
                                <span>{ format!("{}", e) }</span>
                            </div>
                        },
                        Some(Ok(info)) => {
                            let mut sorted_states: Vec<(&String, &Complexity)> = info.estimated_state_complexities
                                .iter()
                                .filter(|(state, _)| !ignored_state_names.contains(&state.to_lowercase()))
                                .collect();

                            sorted_states.sort_by(|a, b| a.0.cmp(b.0));

                            html! {
                                <div style="display: flex; flex-wrap: wrap; gap: 2rem; padding-top: 1rem">
                                    <div style="flex: 1 1 300px; min-width: 0; display: flex; flex-direction: column; gap: 0.75rem;">
                                        <h3 class="font-bold text-lg text-white" style="margin: 0;">{"Total Runtime Graph"}</h3>
                                        <RuntimeChart 
                                            id={"overall-chart".to_string()} 
                                            data={info.graph_data.clone()} 
                                            is_small={false}
                                        />

                                        <h3 class="font-bold text-lg text-white" style="margin-bottom: 0.25rem;">{"Total Time Complexity"}</h3>
                                        <span class="text-primary" style="font-size: 1.5rem; font-weight: bold;">{ format!("{}", info.estimated_complexity) }</span>
                                    </div>
                                    
                                    <div style="flex: 1 1 350px; min-width: 0; display: flex; flex-direction: column; gap: 1.5rem;">
                                        <div class="bg-base-200">
                                            <h3 class="font-bold text-lg text-white" style="margin: 0;">{"State Breakdown"}</h3>
                                        </div>

                                        <div class="bg-base-100 rounded-box" style="display: flex; flex-direction: column; border: 1px solid rgba(128,128,128,0.2); overflow: hidden;">
                                            
                                            <div style="overflow-y: auto; display: flex; flex-direction: column; width: 100%;">
                                                
                                                <div class="bg-base-300 text-white" style="display: flex; width: 100%; border-bottom: 1px solid rgba(128,128,128,0.2);">
                                                    <div style="width: 15%; padding: 0.5rem; font-size: 0.875rem; text-align: center;">{"State"}</div>
                                                    <div style="width: 15%; padding: 0.5rem; font-size: 0.875rem; text-align: center;">{"Time"}</div>
                                                    <div style="width: 70%; padding: 0.5rem; font-size: 0.875rem; text-align: center;">{"Runtime Graph"}</div>
                                                </div>
                                                
                                                <div style="display: flex; flex-direction: column; width: 100%;">
                                                    { for sorted_states.iter().enumerate().map(|(i, (state, comp))| {
                                                        let state_data = info.state_graph_data.get(*state).cloned().unwrap_or_default();

                                                        html! {
                                                            <div key={state.to_string()} class="hover:bg-base-200 transition-colors" style="display: flex; width: 100%; border-bottom: 1px solid rgba(128,128,128,0.2);">
                                                                <div style="width: 15%; padding: 0.5rem; font-family: monospace; font-size: 0.875rem; display: flex; align-items: center; border-right: 1px solid rgba(128,128,128,0.2); overflow: hidden; text-overflow: ellipsis; text-align: center;">
                                                                    { state }
                                                                </div>
                                                                <div class="text-primary" style="width: 15%; padding: 0.5rem; font-weight: bold; font-size: 0.875rem; display: flex; align-items: center; border-right: 1px solid rgba(128,128,128,0.2); overflow: hidden; text-overflow: ellipsis; text-align: center;">
                                                                    { format!("{}", comp) }
                                                                </div>
                                                                <div style="width: 70%; padding: 0; position: relative; display: flex; align-items: center;">
                                                                    <RuntimeChart
                                                                        id={format!("state-chart-{}", i)}
                                                                        data={state_data}
                                                                        is_small={true}
                                                                    />
                                                                </div>
                                                            </div>
                                                        }
                                                    }) }
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
                    <div class="modal-box relative" style="min-width: 50%; max-height: 90%;" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                        <button
                            class="modal-close-btn"
                            onclick={close_modal.clone()}
                            style="position: absolute; top: 8px; right: 8px; display: flex; align-items: center; justify-content: center; font-size: 18px; line-height: 1; z-index: 10;"
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
                            <p class="help-paragraph">{"
                            This analyser currently only supports single-tape Turing Machines.
                            "}</p>

                            <h4 class="help-heading">{"Regular Expressions:"}</h4>
                            <p class="help-paragraph">{"
                            The variant of regular expressions used here has a simplified syntax, and supports different features compared to typical RegEx. For example:
                            "}</p>
                            <ul class="help-list">
                                <li class="help-list-item">{"Generate either '0', '1', or '22': "}<code>{"(0|1|22)"}</code></li>
                                <li class="help-list-item">{"Repeat '0' 10-20 times: "}<code>{"0{10,20}"}</code></li>
                                <li class="help-list-item">{"A simple generator for worst-case binary addition inputs: "} <code>{"$(1)*"}</code></li>
                                <li class="help-list-item">{"A simple even number generator: "}<code>{"1(1|0)*0"}</code></li>
                            </ul>
                            <p class="help-paragraph">
                                {"Ranges (i.e. a-z) are not supported due to them being generally unnecessary during generation - if a-z is accepted, so is just a."}
                            </p>
                            <p class="help-paragraph">
                                {"Multiple expressions can be provided at once. Expressions should be delimited with the ';' character. Additionally, reserved characters (i.e. {, *, +, ;) can be used 
                                as literals through escaping with the '\\' character."}
                            </p>


                            <h4 class="help-heading">{"Advanced Features:"}</h4>
                            <p class="help-paragraph">{"
                            Additionally supported are mathematical operations within ranges, supporting variables which are intelligently assigned during analysis. 
                            This allows for limited context preservation, enabling some more complex Turing Machines to be analysed and partially rectifying the capability gap between 
                            standard RegEx and Turing Machines. For example:
                            "}</p>
                            <ul class="help-list">
                                <li class="help-list-item">{"A generator for outputs of square sizes: "} <code>{"0{n * n}"}</code></li>
                                <li class="help-list-item">{"A generator for outputs with 1 less 'a' than 'b' than 'c': "} <code>{"a{n}b{n + 1}c{n + 2}"}</code></li>
                                <li class="help-list-item">{"A generator with specific bounds on outputs: "} <code>{"a{n}b{n * 2 + x}c{x - n}"}</code></li>
                            </ul>
                            <p class="help-paragraph">{"
                            As such, it can be observed that this language has non-regular capabilities through violating the "}<a href="https://en.wikipedia.org/wiki/Pumping_lemma_for_regular_languages" target="_blank" style="text-decoration: underline; color: rgb(142, 155, 255);">{"pumping lemma for regular languages"}</a>
                            {". There are no specific bounds on the number of variables, but higher numbers of variables and more restrictive bounds may lead to longer processing times or failure to generate outputs.
                            "}</p>
                            <p class="help-paragraph">{"
                            Also given are advanced options next to the Run Analysis button. These function as follows: 
                            "}</p>
                            <ul class="help-list">
                                <li class="help-list-item">{"Analyse Accepted Only: Only adds generated inputs which end in a state containing \"accept\" to the end analysis"}</li>
                                <li class="help-list-item">{"Attempts: Makes this many attempts to generate each string for each length, keeping the string with the highest runtime. Can lead to more accurate worst-case results at the cost of higher analysis time."}</li>
                            </ul>
                        </div>
                    </div>
                    
                    <form method="dialog" class="modal-backdrop" onclick={close_modal.clone()}>
                        <button>{"close"}</button>
                    </form>
                </dialog>
            </div>
        </div>
    }
}