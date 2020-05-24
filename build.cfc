component accessors="true" {

    property filesystem inject="filesystem";

    function run() {
        syntect();
        cftokens();
    }

    function cftokens() {
        var dir = resolvePath( './cftokens/' );
        command( '!cargo build --release' ).run();

        print.line( 'Copying binary to "./bin/" folder...' );
        var srcBinaryName = filesystem.isWindows() ? 'cftokens.exe' : 'cftokens';
        var targetBinaryName = getTargetBinaryName();
        var src = resolvePath( './target/release/#srcBinaryName#' );
        var dest = resolvePath( './bin/' );
        directoryCreate( dest, true, true );
        fileCopy( src, dest & targetBinaryName );

        if ( !filesystem.isWindows() ) {
            print.line( 'Ensuring that it is executable...' );
            command( '!chmod +x "#dest & targetBinaryName#"' ).run();
        }

        print.text( 'Binary is at: ' );
        print.greenLine( dest & targetBinaryName );
    }

    function syntect() {
        var cftokensDir = resolvePath( './' );

        if ( !directoryExists( cftokensDir & 'syntect' ) ) {
            print.line( 'Cloning Syntect repo from GitHub...' ).toConsole();
            command( '!git clone https://github.com/trishume/syntect.git ./syntect' ).inWorkingDirectory( cftokensDir ).run();
        } else {
            print.line( 'Pulling Syntect repo from GitHub...' ).toConsole();
            command( '!git pull' ).inWorkingDirectory( cftokensDir & 'syntect' ).run();
        }

        print.line( 'Cleaning testdata folder...' )
        directoryDelete( cftokensDir & 'syntect/testdata', true );
        directoryCreate( cftokensDir & 'syntect/testdata' );

        print.line( 'Copying syntaxes...' );
        for ( var syntax in [ 'HTML', 'JavaScript', 'SQL', 'CSS' ] ) {
            print.line( 'Copying ' & syntax & '...' ).toConsole();
            directoryCopy(
                cftokensDir & 'Packages/' & syntax,
                cftokensDir & 'syntect/testdata/' & syntax,
                false,
                '*.sublime-syntax'
            );
        }

        print.line( 'Copying CFML...' ).toConsole();
        directoryCopy(
            cftokensDir & 'CFML/',
            cftokensDir & 'syntect/testdata/CFML/',
            false,
            ( p ) => p.contains( 'cfml' ) || p.contains( 'cfscript' )
        );

        print.line();
        print.line( 'Building syntect packs...' ).toConsole();
        command(
            '!cargo run --features=metadata --example gendata -- synpack testdata assets/default_newlines.packdump assets/default_nonewlines.packdump assets/default_metadata.packdump'
        ).inWorkingDirectory( cftokensDir & 'syntect/' ).run();
    }

    function tokens( string path ) {
        var dir = resolvePath( './' );
        var binary = getTargetBinaryName();
        var lf = filesystem.isWindows() ? chr( 13 ) & chr( 10 ) : chr( 10 );

        // generate tokens
        var tokenjson = '';
        cfexecute(
            name=expandPath( dir & "bin/#binary#" ),
            arguments="parse ""#resolvePath( path )#""",
            variable="tokenjson",
            timeout=10
        );

        print.line( deserializeJSON( tokenjson ) );
    }

    function getTargetBinaryName() {
        if ( filesystem.isWindows() ) return 'cftokens.exe';
        if ( filesystem.isMac() ) return 'cftokens_osx';
        if ( filesystem.isLinux() ) return 'cftokens_linux';
    }

}
