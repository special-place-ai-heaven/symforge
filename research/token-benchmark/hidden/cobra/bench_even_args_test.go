package cobra

import "testing"

func TestBenchEvenArgs(t *testing.T) {
	c := &Command{Use: "c"}
	for _, ok := range [][]string{{}, {"a", "b"}, {"a", "b", "c", "d"}} {
		if err := EvenArgs(c, ok); err != nil {
			t.Fatalf("EvenArgs(%v) = %v, want nil", ok, err)
		}
	}
	for _, bad := range [][]string{{"a"}, {"a", "b", "c"}} {
		if err := EvenArgs(c, bad); err == nil {
			t.Fatalf("EvenArgs(%v) = nil, want error", bad)
		}
	}
}
