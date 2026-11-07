# Behaviour specifications

Written from black-box observation of the original HTerm (running it against
virtual serial ports, strace/ltrace, sample files). Describe *what* it does,
never how its code is written. One file per area:

- [connection.md](connection.md): port list, settings, open/close, status lines
- [views.md](views.md): window layout, data views, counters
- [newline.md](newline.md): newline display rules
- [input.md](input.md): input line tokens, sending
- [file-formats.md](file-formats.md): `hterm.cfg`, mfs notation, `.hts`
- [open-questions.md](open-questions.md): what is still unknown

Add new areas as new files and link them here.
