use yew::prelude::*;
use plotters::prelude::*;
use plotters_canvas::CanvasBackend;
use web_sys::HtmlCanvasElement;
use tur::Program;
use tur::expression::analyse_expression;
use tur::expression::{AnalysisInfo, Complexity};

#[derive(Properties, PartialEq)]
pub struct AnalyzerProps {
    pub program: Program,
}

#[derive(Properties, PartialEq)]
pub struct ChartProps {
    pub data: Vec<(usize, usize)>,
}

pub fn draw_complexity_chart(canvas: HtmlCanvasElement, data: &[(usize, usize)]) -> Result<(), Box<dyn std::error::Error>> {
    let backend = CanvasBackend::with_canvas_object(canvas).unwrap();
    let root = backend.into_drawing_area();
    root.fill(&WHITE)?;

    if data.is_empty() { return Ok(()); }

    let max_x = data.iter().map(|(x, _)| *x).max().unwrap_or(10) as f64;
    let max_y = data.iter().map(|(_, y)| *y).max().unwrap_or(10) as f64;

    let mut chart = ChartBuilder::on(&root)
        .margin(15)
        .x_label_area_size(40)
        .y_label_area_size(50)
        .build_cartesian_2d(0f64..(max_x * 1.05), 0f64..(max_y * 1.05))?;

    chart.configure_mesh()
        .x_desc("Input Length (N)")
        .y_desc("Turing Machine Steps")
        .label_style(("sans-serif", 14))
        .axis_desc_style(("sans-serif", 15, &BLACK))
        .draw()?;

    let data_points: Vec<(f64, f64)> = data.iter().map(|(x, y)| (*x as f64, *y as f64)).collect();

    chart.draw_series(LineSeries::new(data_points.clone(), &BLUE))?;
    chart.draw_series(
        data_points.iter().map(|point| Circle::new(*point, 4, &BLUE.mix(0.8)))
    )?;

    root.present()?;
    Ok(())
}

#[function_component(RuntimeChart)]
fn runtime_chart(props: &ChartProps) -> Html {
    let canvas_ref = use_node_ref();

    {
        let canvas_ref = canvas_ref.clone();
        let data = props.data.clone();

        use_effect_with(data.clone(), move |deps| {
            if let Some(canvas) = canvas_ref.cast::<HtmlCanvasElement>() {
                let _ = draw_complexity_chart(canvas, deps); 
            }
            || ()
        });
    }

    html! {
        <div class="w-full flex justify-center bg-white rounded-box shadow-sm border border-base-300 p-2">
            <canvas ref={canvas_ref} width="600" height="350" style="max-width: 100%;"></canvas>
        </div>
    }
}

#[function_component(ComplexityAnalyser)]
pub fn complexity_analyser(props: &AnalyzerProps) -> Html {
    let regex_input = use_state(|| "(a|b){1, 15}".to_string());
    let is_analyzing = use_state(|| false);
    let analysis_result = use_state(|| None::<Result<AnalysisInfo, String>>);

    let on_input_change = {
        let regex_input = regex_input.clone();
        Callback::from(move |e: InputEvent| {
            let input = e.target_unchecked_into::<web_sys::HtmlInputElement>();
            regex_input.set(input.value());
        })
    };

    let on_analyze_click = {
        let regex_input = regex_input.clone();
        let is_analyzing = is_analyzing.clone();
        let analysis_result = analysis_result.clone();
        let program = props.program.clone();

        Callback::from(move |_| {
            if regex_input.is_empty() { return; }

            is_analyzing.set(true);
            analysis_result.set(None);

            let async_regex = regex_input.clone();
            let async_analyzing = is_analyzing.clone();
            let async_result = analysis_result.clone();
            let async_prog = program.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let _ = gloo_timers::future::sleep(std::time::Duration::from_millis(15)).await;
                let result = analyse_expression(&async_regex, &async_prog);
                
                async_result.set(Some(result));
                async_analyzing.set(false);
            });
        })
    };

    html! {
        <div class="card card-compact bg-base-100 mt-4 shadow-md border border-base-200">
            <div class="card-body">
                <h3 class="card-title text-lg border-b border-base-200 pb-2">{"Empirical Complexity Analysis"}</h3>
                
                <div class="flex flex-col sm:flex-row items-center gap-3 bg-base-200/50 p-3 rounded-box border border-base-200">
                    <input 
                        type="text" 
                        class="input input-bordered w-full font-mono text-sm shadow-inner" 
                        value={(*regex_input).clone()}
                        oninput={on_input_change}
                        disabled={*is_analyzing}
                        placeholder="Enter Regex (e.g., (a|b){1,20})"
                    />
                    <button 
                        class="btn btn-primary w-full sm:w-auto px-8"
                        onclick={on_analyze_click} 
                        disabled={*is_analyzing || regex_input.is_empty()}
                    >
                        { if *is_analyzing { html!{<span class="loading loading-spinner loading-sm"></span>} } else { html!{} } }
                        { if *is_analyzing { "Simulating..." } else { "Run Analysis" } }
                    </button>
                </div>

                {
                    match &*analysis_result {
                        None => html! {},
                        Some(Err(e)) => html! {
                            <div class="alert alert-error shadow-sm mt-4">
                                <svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
                                <span>{ format!("Error: {}", e) }</span>
                            </div>
                        },
                        Some(Ok(info)) => html! {
                            <div class="grid grid-cols-1 lg:grid-cols-3 gap-6 mt-4">
                                <div class="col-span-1 lg:col-span-2">
                                    <RuntimeChart data={info.graph_data.clone()} />
                                </div>
                                
                                <div class="col-span-1 flex flex-col gap-4">
                                    <div class="stat bg-base-200 rounded-box border border-base-300 shadow-sm">
                                        <div class="stat-title font-semibold text-base-content/80">{"Global Complexity"}</div>
                                        <div class="stat-value text-primary text-3xl">{ format!("O({:?})", info.estimated_complexity) }</div>
                                    </div>
                                    
                                    <div class="flex-grow overflow-hidden rounded-box border border-base-300 bg-base-100 shadow-sm">
                                        <table class="table table-zebra table-sm w-full">
                                            <thead class="bg-base-200 text-base-content">
                                                <tr>
                                                    <th>{"State"}</th>
                                                    <th class="text-right">{"O(n)"}</th>
                                                </tr>
                                            </thead>
                                            <tbody>
                                                { for info.estimated_state_complexities.iter().map(|(state, comp)| html! {
                                                    <tr>
                                                        <td class="font-mono text-xs">{ state }</td>
                                                        <td class="font-bold text-right text-secondary">{ format!("{:?}", comp) }</td>
                                                    </tr>
                                                }) }
                                            </tbody>
                                        </table>
                                    </div>
                                </div>
                            </div>
                        }
                    }
                }
            </div>
        </div>
    }
}