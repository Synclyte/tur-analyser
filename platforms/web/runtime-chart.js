// js intermediate layer between chart.js and rust - provides a function to be called by rust
window.chartInstances = window.chartInstances || {};

function addStringTooltips(stringArray) {
  return function(context) {
    const index = context.dataIndex;
    const inputString = stringArray[index];

    // if the label is provided with an array of strings, it displays them line-by-line
    // this allows for the full input string to always be displayed within reasonable horizontal space
    if (inputString && inputString !== "") {
      const lineLength = 30;
      const lines = []
      for (let i = 0; i < inputString.length; i += lineLength) {
        lines.push(inputString.slice(i, i + lineLength));
      }

      const formattedLines = []
      for (let i = 0; i < lines.length; i++) {
        if (i === 0 && lines.length === 1) formattedLines.push(`Input: "${lines[i]}"`)
        else if (i === 0) formattedLines.push(`Input: "${lines[i]}`)
        else if (i === lines.length - 1) formattedLines.push(`        ${lines[i]}"`)
        else formattedLines.push(`        ${lines[i]}`)
      }

      return formattedLines;
    }
    return null;
  }
}

export function draw_runtime_chart(canvas_id, x_data, y_data, string_data, is_small) {
  const ctx = document.getElementById(canvas_id);
  if (!ctx) return;
  if (window.chartInstances[canvas_id]) window.chartInstances[canvas_id].destroy();

  const style = getComputedStyle(document.body);

  // pulls colours from style.css to be used here
  const primaryColour = style.getPropertyValue('--text-color');
  const textColour = style.getPropertyValue('--text-color');
  const gridColour = style.getPropertyValue('--background-color');
  const fontMono = style.getPropertyValue('--font-family-mono');

  if (!is_small) {
    window.chartInstances[canvas_id] = new Chart(ctx, {
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
          pointHoverRadius: 7,
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
            // a callback is necessary due to it being impossible to store tooltip strings as a chart.js property
            callbacks: {
              afterLabel: addStringTooltips(string_data)
            },
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
  } else {
    window.chartInstances[canvas_id] = new Chart(ctx, {
      type: 'line',
      data: {
        labels: x_data,
        datasets: [{
          label: 'Turing Machine Steps',
          data: y_data,
          borderColor: primaryColour,
          backgroundColor: primaryColour,
          tension: 0.2, // determines line curvature
          pointRadius: 2,
          pointHoverRadius: 7,
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
            callbacks: {
              afterLabel: addStringTooltips(string_data)
            },
            backgroundColor: 'rgba(0, 0, 0, 0.8)',
            titleFont: { family: fontMono },
            bodyFont: { family: fontMono },
          }
        },
      }
    });   
  }

}