local extension = app.params["extension"]
if not extension or extension == "" then
  error("Missing extension path")
end

app.command.Options {
  installExtension = extension
}
