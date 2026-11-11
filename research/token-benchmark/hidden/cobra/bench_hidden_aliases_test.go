package cobra

import (
	"bytes"
	"strings"
	"testing"
)

func TestBenchHiddenAliases(t *testing.T) {
	ran := false
	root := &Command{Use: "root", Run: func(*Command, []string) {}}
	child := &Command{
		Use:           "child",
		Aliases:       []string{"ch"},
		HiddenAliases: []string{"hx"},
		Run:           func(*Command, []string) { ran = true },
	}
	root.AddCommand(child)

	root.SetArgs([]string{"hx"})
	if _, err := root.ExecuteC(); err != nil || !ran {
		t.Fatalf("hidden alias did not run the child: err=%v ran=%v", err, ran)
	}
	if !child.HasAlias("hx") {
		t.Fatal("HasAlias(hx) = false")
	}
	if strings.Contains(child.NameAndAliases(), "hx") {
		t.Fatalf("NameAndAliases shows the hidden alias: %q", child.NameAndAliases())
	}

	buf := new(bytes.Buffer)
	root.SetOut(buf)
	root.SetErr(buf)
	root.SetArgs([]string{"child", "--help"})
	_ = root.Execute()
	if strings.Contains(buf.String(), "hx") {
		t.Fatalf("help output shows the hidden alias:\n%s", buf.String())
	}
}
