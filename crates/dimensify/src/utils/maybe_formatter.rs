// based on https://stackoverflow.com/questions/64955738/generic-implementation-depending-on-traits

struct MaybeFormat<T>(T);

trait Formatter {
    fn maybe_format(&self) -> String;
}

// specialized implementation
impl<T: std::fmt::Display> Formatter for MaybeFormat<T> {
    fn maybe_format(&self) -> String {
        format!("{}", self.0)
    }
}

trait DefaultFormatter {
    fn maybe_format(&self) -> String;
}

// default implementation
//
// Note that the Self type of this impl is &MaybeFormat<T> and so the
// method argument is actually &&T!
// That makes this impl lower priority during method
// resolution than the implementation for `Formatter` above.
impl<T> DefaultFormatter for &MaybeFormat<T> {
    fn maybe_format(&self) -> String {
        format!("I cannot be printed")
    }
}

struct NotDisplay;

#[macro_export]
macro_rules! maybe_format {
    ($e:expr) => {
        (&$e).maybe_format()
    };
}

// fn main() {
//     let not_printable = MaybeFormat(NotDisplay);
//     let printable = MaybeFormat("Hello World");

//     println!("{}", maybe_format!(&not_printable));
//     maybe_format!(&printable);
// }

// => I cannot be printed
// => Hello World
