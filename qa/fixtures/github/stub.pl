#!/usr/bin/perl
# qa/fixtures/github/stub.pl — the three GitHub routes the authority's device flow calls.
#
# The authority reads GITHUB_OAUTH_BASE and GITHUB_API_BASE, so a battery can point it here and
# drive `42ctl auth login --github` to the end without a GitHub account. The person's side of
# the flow is files in STUB_DIR: `approve` makes the token exchange succeed, `deny` makes GitHub
# refuse the grant, and `emails.json` is what `/user/emails` answers. Until one of the first two
# exists every exchange answers `authorization_pending`, which is what GitHub says while nobody
# has typed the code yet.
#
# Every request is appended to STUB_DIR/requests.log as `METHOD PATH ANSWER`, plus the facts a
# spec asserts on (the client id sent, whether the token came back as a bearer). No token value
# is logged. Written for perl-base alone, which every Debian image carries.
use strict;
use warnings;
use IO::Socket::INET;

my $dir = $ENV{STUB_DIR} || '/stub';
my $server = IO::Socket::INET->new(LocalPort => 8080, Listen => 16, ReuseAddr => 1)
	or die "cannot listen on 8080: $!";
$| = 1;
print "listening\n";

while (my $client = $server->accept) {
	my ($method, $path, $headers, $body) = request($client);
	next unless defined $method;
	my ($status, $json, $note) = answer($method, $path, $headers, $body);
	if (open my $log, '>>', "$dir/requests.log") {
		print $log "$method $path $note\n";
		close $log;
	}
	print $client "HTTP/1.1 $status\r\nContent-Type: application/json\r\n"
		. 'Content-Length: ' . length($json) . "\r\nConnection: close\r\n\r\n$json";
	close $client;
}

# Read one HTTP/1.1 request: its method, path, lower-cased headers and body.
sub request {
	my ($client) = @_;
	my $line = <$client>;
	return unless defined $line;
	my ($method, $path) = split ' ', $line;
	my %headers;
	while (my $header = <$client>) {
		last if $header =~ /^\r?\n$/;
		$headers{lc $1} = $2 if $header =~ /^([^:]+):\s*(.*?)\r?\n$/;
	}
	my $body = '';
	read($client, $body, $headers{'content-length'}) if $headers{'content-length'};
	return ($method, $path, \%headers, $body);
}

# The answer GitHub would give, and a note on what was asked.
sub answer {
	my ($method, $path, $headers, $body) = @_;
	if ($method eq 'POST' && $path eq '/login/device/code') {
		my ($client_id) = $body =~ /(?:^|&)client_id=([^&]*)/;
		return ('200 OK', '{"device_code":"qa-device-code","user_code":"QA42-0001",'
			. '"verification_uri":"https://github.com/login/device","expires_in":60,"interval":1}',
			'client_id=' . ($client_id // ''));
	}
	if ($method eq 'POST' && $path eq '/login/oauth/access_token') {
		return ('200 OK', '{"error":"access_denied"}', 'denied') if -e "$dir/deny";
		return ('200 OK', '{"access_token":"qa-github-token","token_type":"bearer"}', 'token')
			if -e "$dir/approve";
		return ('200 OK', '{"error":"authorization_pending"}', 'pending');
	}
	if ($method eq 'GET' && $path eq '/user/emails') {
		my $bearer = ($headers->{authorization} // '') eq 'Bearer qa-github-token' ? 'bearer-ok' : 'bearer-wrong';
		open my $file, '<', "$dir/emails.json" or return ('404 Not Found', '{"message":"Not Found"}', $bearer);
		local $/;
		return ('200 OK', scalar <$file>, $bearer);
	}
	return ('404 Not Found', '{"message":"Not Found"}', 'unknown');
}
