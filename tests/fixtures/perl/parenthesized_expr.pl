package Calc;

sub total {
    my ($self, @items) = @_;
    my $sum = (helper(1) + $self->scale());
    my $ok = (valid($sum) && ($cb->()));
    return (Calc::Util::round($sum));
}
