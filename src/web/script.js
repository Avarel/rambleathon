// https://cdnjs.cloudflare.com/ajax/libs/codemirror/5.30.0/mode/coffeescript/coffeescript.js fork this

let textArea = $('#ramble textarea').text(
`Loading the ramblathon...`
);

let editor = CodeMirror.fromTextArea(textArea[0], {
  theme: "twilight",
  lineNumbers: true,
  lineWrapping: true,
  electricChars: false,
  smartIndent: false,
  scrollbarStyle: "null"
});

editor.setOption("extraKeys", {
  Backspace: cm => {},
  Delete: cm => {},
  Enter: cm => cm.replaceSelection("\n"),
});

editor.on("cursorActivity", function(event) {
  editor.execCommand("goDocEnd");
});
editor.doc.readOnly = true;

let batch = "";

editor.on("changes", function(instance, changes) {
  console.log(changes);
  for (let change of changes) {
    batch += change.text[0];
    for (let i = 1; i < change.text.length; i++) {
      batch += '\n' + change.text[i];
    }
  }
});

let ws = null;

function connect() {
  ws = new WebSocket("ws://localhost:42069/ws");

  ws.onmessage = function(event) {
    console.log(event);
    editor.doc.setValue(event.data);
    batch = "";
    editor.doc.readOnly = false;
  };
  
  ws.onclose = function(event) {
    editor.doc.setValue("Ramblathon is currently closed!");
    editor.doc.readOnly = true;
    batch = "";
    ws = null; 
  }
  batch = "";
}

setInterval(function() {
  if (ws != null && batch.length > 0) {
    ws.send(batch);
    batch = "";
  }
}, 5000);

setInterval(function() {
  if (ws == null) {
    connect();
  }
}, 10000);

connect();