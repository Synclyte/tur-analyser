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
            let x_data: Vec<f64> = points.iter().map(|(x, _)| *x as f64).collect();
            let y_data: Vec<f64> = points.iter().map(|(_, y)| *y as f64).collect();
            
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
                <button 
                    class="btn btn-primary px-8 w-full sm:w-auto justify-center"
                    onclick={on_analyse_click} 
                    disabled={*is_analysing || regex_input.is_empty()}
                >
                    { if *is_analysing { "Simulating Machine" } else { "Run Analysis" } }
                </button>

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
                                .filter(|(state, _)| *state != "start" && *state != "stop") // "start" and "stop" are specifically blacklisted here, as they are always only visited once
                                .collect();

                            // sorts alphabetically to maintain order consistency across analysis runs
                            sorted_states.sort_by(|a, b| a.0.cmp(b.0));

                            html! {
                                <div 
                                    class="grid grid-cols-1 lg:grid-cols-3 gap-4 mt-4"
                                    style="padding-top: 30px;"
                                    >
                                    <div class="col-span-1 lg:col-span-2">
                                        <RuntimeChart data={info.graph_data.clone()} />
                                    </div>
                                    
                                    <div class="col-span-1 flex flex-col gap-4 h-full">
                                        <div class="bg-base-200 rounded-box p-5 flex flex-col justify-center items-center border border-base-300 shadow-sm">
                                            <span class="text-xs font-bold uppercase tracking-wider text-base-content/60">{"Global Complexity"}</span>
                                            <span class="text-4xl font-black text-primary mt-2">{ format!("O({:?})", info.estimated_complexity) }</span>
                                        </div>
                                        
                                        <div class="flex-grow overflow-hidden rounded-box border border-base-300 bg-base-100 shadow-sm flex flex-col">
                                            <div class="bg-base-200 px-4 py-3 border-b border-base-300 flex justify-between items-center">
                                                <span class="text-xs font-bold uppercase tracking-wider text-base-content/60">{"State Breakdown"}</span>
                                                <span class="badge badge-sm badge-outline">{ sorted_states.len() }</span>
                                            </div>
                                            <div class="overflow-y-auto" style="max-height: 250px;">
                                                <table class="table table-zebra table-sm w-full">
                                                    <tbody>
                                                        { for sorted_states.iter().map(|(state, comp)| html! {
                                                            <tr>
                                                                <td class="font-mono text-sm pl-4">{ state }</td>
                                                                <td class="font-bold text-right text-secondary pr-4">{ format!("{:?}", comp) }</td>
                                                            </tr>
                                                        }) }
                                                    </tbody>
                                                </table>
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
                        
                        <h3 class="font-bold text-lg mb-4">{"Complexity Analysis"}</h3>
                        <div class="space-y-4 text-base-content/80">
                            <p>{"help information here"}</p>
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