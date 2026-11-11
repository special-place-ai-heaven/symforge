package cobra

import "testing"

func TestBenchMinimumNArgsBoundary(t *testing.T) {
	v := MinimumNArgs(2)
	c := &Command{Use: "c"}
	if err := v(c, []string{"a", "b"}); err != nil {
		t.Fatalf("exactly n args rejected: %v", err)
	}
	if err := v(c, []string{"a"}); err == nil {
		t.Fatal("n-1 args accepted")
	}
}
