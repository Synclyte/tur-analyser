// js intermediate layer between plot.js and rust - provides a function to be called by rust
let runtimeChart = null;

export function draw_runtime_chart(canvas_id, x_data, y_data) {
  const ctx = document.getElementById(canvas_id);
  if (!ctx) return;
  if (runtimeChart) runtimeChart.destroy();

  const style = getComputedStyle(document.body);

  // pulls colours from style.css to be used here
  const primaryColour = style.getPropertyValue('--text-color');
  const textColour = style.getPropertyValue('--text-color');
  const gridColour = style.getPropertyValue('--background-color');
  const fontMono = style.getPropertyValue('--font-family-mono');

  runtimeChart = new Chart(ctx, {
    type: 'line',
    data: {
      labels: x_data,
      datasets: [{
        label: 'Turing Machine Steps',
        data: y_data,
        borderColor: primaryColour,
        backgroundColor: primaryColour,
        tension: 0.4, // determines line curvature
        pointRadius: 3,
        pointHoverRadius: 6,
        spanGaps: true,
      }]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      color: textColour,
      plugins: {
        legend: { display: false },
        tooltip: {
          backgroundColor: 'rgba(0, 0, 0, 0.8)',
          titleFont: { family: fontMono },
          bodyFont: { family: fontMono },
        }
      },
      scales: {
        x: {
          title: { display: true, text: 'Input Length (N)', color: textColour },
          grid: { color: gridColour },
          ticks: { color: textColour }
        },
        y: {
          title: { display: true, text: 'Steps', color: textColour },
          grid: { color: gridColour },
          ticks: { color: textColour }
        }
      }
    }
  });
}